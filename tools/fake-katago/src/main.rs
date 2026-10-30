mod cli;
mod model;

use std::fs::OpenOptions;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use cli::{Options, USAGE};
use model::Query;
use serde_json::{Value, json};
use thiserror::Error;

#[derive(Debug, Error)]
enum RunError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("injected crash")]
    Crash,
    #[error("模型找不到")]
    MissingModel,
    #[error("could not read startup gate {}: {source}", path.display())]
    StartupGate {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not claim fault-once marker {}: {source}", path.display())]
    FaultMarker {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

struct Job {
    query: Query,
    turn: usize,
    priority: i64,
    sequence: u64,
}

struct Running {
    job: Job,
    start: f64,
    next_report: f64,
}

struct Engine<W: Write> {
    options: Options,
    output: W,
    started: Instant,
    pending: Vec<Job>,
    running: Vec<Running>,
    sequence: u64,
    queries: u64,
    lines: u64,
    hung: bool,
}

impl<W: Write> Engine<W> {
    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * self.options.time_scale
    }

    fn emit(&mut self, value: Value) -> Result<(), RunError> {
        if self.hung {
            return Ok(());
        }
        serde_json::to_writer(&mut self.output, &value)?;
        writeln!(self.output)?;
        self.lines = self.lines.wrapping_add(1);
        if self
            .options
            .garbage_every
            .is_some_and(|n| self.lines.is_multiple_of(n))
        {
            writeln!(self.output, "this is deliberately not JSON")?;
        }
        self.output.flush()?;
        Ok(())
    }

    fn error(&mut self, value: &Value, field: &str, message: &str) -> Result<(), RunError> {
        let mut response = json!({"error": message, "field": field});
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            response["id"] = json!(id);
        }
        self.emit(response)
    }

    fn receive(&mut self, line: &str) -> Result<(), RunError> {
        if self.hung {
            return Ok(());
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => return self.emit(json!({"error": error.to_string()})),
        };
        if !value.is_object() {
            return self.emit(json!({"error": "Expected a JSON object"}));
        }
        let Some(id) = value["id"].as_str() else {
            return self.error(&value, "id", "Expected a string id");
        };
        if let Some(action) = value.get("action") {
            match action.as_str() {
                Some("query_version") => self.emit(json!({
                    "id": id, "action": "query_version", "version": "1.16.5-fake", "git_hash": "fake-katago"
                }))?,
                Some("terminate") => {
                    let Some(target) = value["terminateId"].as_str() else {
                        return self.error(&value, "terminateId", "Expected a string terminateId");
                    };
                    self.terminate(Some(target), json!({"id": id, "action": "terminate", "terminateId": target}))?;
                }
                Some("terminate_all") => self.terminate(None, json!({"id": id, "action": "terminate_all"}))?,
                _ => self.error(&value, "action", "Unknown action")?,
            }
            return Ok(());
        }
        let query = match Query::parse(&value) {
            Ok(query) => query,
            Err(error) => return self.error(&value, error.field, &error.message),
        };
        self.queries = self.queries.saturating_add(1);
        if self
            .options
            .crash_after_queries
            .is_some_and(|n| self.queries >= n)
            && claim_fault(self.options.fault_once_file.as_deref())?
        {
            return Err(RunError::Crash);
        }
        if self
            .options
            .hang_after_queries
            .is_some_and(|n| self.queries >= n)
            && claim_fault(self.options.fault_once_file.as_deref())?
        {
            self.hung = true;
            self.running.clear();
            self.pending.clear();
            return Ok(());
        }
        if let Some(fields) = value.as_object() {
            for field in fields
                .keys()
                .filter(|key| !model::FIELDS.contains(&key.as_str()))
            {
                self.emit(json!({
                    "id": id, "field": field,
                    "warning": "Unexpected or unused field, do you have a typo? (set warnUnusedFields=false in the config to disable this warning)"
                }))?;
            }
        }
        for (&turn, &priority) in query.turns.iter().zip(&query.priorities) {
            self.pending.push(Job {
                query: query.clone(),
                turn,
                priority,
                sequence: self.sequence,
            });
            self.sequence = self.sequence.saturating_add(1);
        }
        Ok(())
    }

    fn terminate(&mut self, target: Option<&str>, acknowledgement: Value) -> Result<(), RunError> {
        let matches = |job: &Job| target.is_none_or(|id| job.query.id == id);
        // The recorded engine reports queued noResults before the action acknowledgement.
        let queued = std::mem::take(&mut self.pending);
        for job in queued {
            if matches(&job) {
                self.emit(json!({"id": job.query.id, "turnNumber": job.turn, "isDuringSearch": false, "noResults": true}))?;
            } else {
                self.pending.push(job);
            }
        }
        self.emit(acknowledgement)?;
        let running = std::mem::take(&mut self.running);
        let now = self.now();
        for search in running {
            if matches(&search.job) {
                let visits = self.visits(&search, now);
                self.emit(search.job.query.response(
                    search.job.turn,
                    visits,
                    false,
                    self.options.perspective(),
                ))?;
            } else {
                self.running.push(search);
            }
        }
        Ok(())
    }

    fn visits(&self, search: &Running, now: f64) -> u64 {
        (((now - search.start).max(0.0) * self.options.visits_per_sec) as u64)
            .max(1)
            .min(search.job.query.max_visits)
    }

    fn start_pending(&mut self) {
        while !self.hung && self.running.len() < self.options.threads {
            let Some((index, _)) = self.pending.iter().enumerate().max_by(|(_, a), (_, b)| {
                a.priority
                    .cmp(&b.priority)
                    .then_with(|| b.sequence.cmp(&a.sequence))
            }) else {
                break;
            };
            let job = self.pending.remove(index);
            let start = self.now();
            let next_report = if job.query.interval.is_some() {
                start + job.query.first_report
            } else {
                f64::INFINITY
            };
            self.running.push(Running {
                job,
                start,
                next_report,
            });
        }
    }

    fn search_deadline(&self, search: &Running) -> f64 {
        search.start + search.job.query.max_visits as f64 / self.options.visits_per_sec
    }

    fn next_event(&self) -> Option<(usize, f64)> {
        self.running
            .iter()
            .enumerate()
            .map(|(i, search)| (i, search.next_report.min(self.search_deadline(search))))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    fn advance(&mut self) -> Result<(), RunError> {
        let Some((index, due)) = self.next_event() else {
            return Ok(());
        };
        if due > self.now() {
            return Ok(());
        }
        let mut search = self.running.remove(index);
        if self.search_deadline(&search) <= search.next_report {
            self.emit(search.job.query.response(
                search.job.turn,
                search.job.query.max_visits,
                false,
                self.options.perspective(),
            ))?;
        } else {
            self.emit(search.job.query.response(
                search.job.turn,
                self.visits(&search, due),
                true,
                self.options.perspective(),
            ))?;
            let previous = search.next_report;
            search.next_report += search.job.query.interval.unwrap_or(f64::INFINITY);
            if search.next_report <= previous {
                search.next_report = f64::INFINITY;
            }
            self.running.insert(index, search);
        }
        Ok(())
    }

    fn startup_gate_released(&self) -> Result<bool, RunError> {
        let Some(path) = &self.options.startup_gate_file else {
            return Ok(true);
        };
        match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => Ok(true),
            Ok(_) => Err(RunError::StartupGate {
                path: path.clone(),
                source: io::Error::new(io::ErrorKind::InvalidInput, "gate is not a regular file"),
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(RunError::StartupGate {
                path: path.clone(),
                source,
            }),
        }
    }

    fn check_timed_crash(&self, deadline: &mut Option<f64>) -> Result<(), RunError> {
        if deadline.is_some_and(|due| self.now() >= due) {
            // An already-claimed marker disables this timer permanently in the
            // current process; retaining its expired deadline would spin.
            *deadline = None;
            if claim_fault(self.options.fault_once_file.as_deref())? {
                return Err(RunError::Crash);
            }
        }
        Ok(())
    }

    fn run(&mut self, input: Receiver<io::Result<String>>) -> Result<(), RunError> {
        let startup = self.options.startup_delay_ms as f64 / 1000.0;
        let mut crash = self.options.crash_after_ms.map(|ms| ms as f64 / 1000.0);
        let mut eof = false;
        // The input reader can queue lines, but no requests are consumed before readiness.
        loop {
            self.check_timed_crash(&mut crash)?;
            let now = self.now();
            if now >= startup && self.startup_gate_released()? {
                break;
            }
            let due = if now < startup {
                startup
            } else {
                f64::INFINITY
            };
            let mut wait = self.wait_duration(due.min(crash.unwrap_or(f64::INFINITY)));
            if self.options.startup_gate_file.is_some() {
                wait = wait.min(Duration::from_millis(10));
            }
            std::thread::park_timeout(wait);
        }
        if self.options.crash_after_queries == Some(0)
            && claim_fault(self.options.fault_once_file.as_deref())?
        {
            return Err(RunError::Crash);
        }
        if self.options.hang_after_queries == Some(0)
            && claim_fault(self.options.fault_once_file.as_deref())?
        {
            self.hung = true;
        }
        loop {
            self.check_timed_crash(&mut crash)?;
            self.start_pending();
            self.advance()?;
            self.start_pending();
            if eof && !self.hung && self.running.is_empty() && self.pending.is_empty() {
                return Ok(());
            }
            let next = self
                .next_event()
                .map(|(_, time)| time)
                .unwrap_or(f64::INFINITY)
                .min(crash.unwrap_or(f64::INFINITY));
            if eof {
                std::thread::park_timeout(self.wait_duration(next));
                continue;
            }
            match input.recv_timeout(self.wait_duration(next)) {
                Ok(line) => self.receive(&line?)?,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => eof = true,
            }
        }
    }

    fn wait_duration(&self, deadline: f64) -> Duration {
        let seconds = ((deadline - self.now()) / self.options.time_scale).clamp(0.0, 86_400.0);
        // The clamped finite range is always representable; fallback also avoids a panic.
        Duration::try_from_secs_f64(seconds).unwrap_or(Duration::from_secs(86_400))
    }
}

fn claim_fault(marker: Option<&Path>) -> Result<bool, RunError> {
    let Some(path) = marker else {
        return Ok(true);
    };
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            match std::fs::metadata(path) {
                Ok(metadata) if metadata.is_file() => Ok(false),
                Ok(_) => Err(RunError::FaultMarker {
                    path: path.to_owned(),
                    source: io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "marker is not a regular file",
                    ),
                }),
                Err(source) => Err(RunError::FaultMarker {
                    path: path.to_owned(),
                    source,
                }),
            }
        }
        Err(source) => Err(RunError::FaultMarker {
            path: path.to_owned(),
            source,
        }),
    }
}

fn run(options: Options) -> Result<(), RunError> {
    if options.fail_startup && claim_fault(options.fault_once_file.as_deref())? {
        return Err(RunError::MissingModel);
    }
    let (sender, receiver) = mpsc::sync_channel(64);
    std::thread::Builder::new()
        .name("fake-katago-stdin".into())
        .spawn(move || {
            for line in io::stdin().lock().lines() {
                if sender.send(line).is_err() {
                    break;
                }
            }
        })?;
    let stdout = io::stdout();
    Engine {
        options,
        output: stdout.lock(),
        started: Instant::now(),
        pending: Vec::new(),
        running: Vec::new(),
        sequence: 0,
        queries: 0,
        lines: 0,
        hung: false,
    }
    .run(receiver)
}

fn main() -> std::process::ExitCode {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(Some(options)) => options,
        Ok(None) => {
            println!("{USAGE}");
            return std::process::ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("{error}");
            return std::process::ExitCode::from(2);
        }
    };
    match run(options) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
