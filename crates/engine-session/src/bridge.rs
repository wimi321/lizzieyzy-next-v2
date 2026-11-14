//! Serial GTP-to-analysis adapter, driven by the same owning session actor as
//! its underlying transport. No detached process task or second engine owner.
use crate::{
    SessionConfig,
    transport::{self, Output, Transport},
};
use gtp::{Decoder, Response, kata_analyze::Report};
use katago_protocol::{Action, AnalysisQuery, Request};
use serde_json::{Value, json};
use std::{collections::VecDeque, future::pending};
use tokio::{
    sync::{mpsc, oneshot},
    time::{Instant, sleep_until},
};
mod query;
mod report;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BridgeError {
    #[error("zhizi bridge: invalid analysis query")]
    InvalidQuery,
    #[error("zhizi bridge: set_position is required for setup stones")]
    SetupUnsupported,
    #[error("zhizi bridge: GTP command rejected")]
    CommandRejected,
    #[error("zhizi bridge: malformed analysis report")]
    MalformedReport,
}
impl BridgeError {
    fn code(&self) -> &'static str {
        match self {
            Self::InvalidQuery => "zhiziBridge.InvalidQuery",
            Self::SetupUnsupported => "zhiziBridge.SetupUnsupported",
            Self::CommandRejected => "zhiziBridge.CommandRejected",
            Self::MalformedReport => "zhiziBridge.MalformedReport",
        }
    }
    pub(crate) fn from_code(code: Option<&str>) -> Option<Self> {
        match code? {
            "zhiziBridge.InvalidQuery" => Some(Self::InvalidQuery),
            "zhiziBridge.SetupUnsupported" => Some(Self::SetupUnsupported),
            "zhiziBridge.CommandRejected" => Some(Self::CommandRejected),
            "zhiziBridge.MalformedReport" => Some(Self::MalformedReport),
            _ => None,
        }
    }
}
struct Queued {
    query: Box<AnalysisQuery>,
    turns: VecDeque<u32>,
    ordinal: u64,
}
struct Active {
    queued: Queued,
    turn: u32,
    setup: VecDeque<String>,
    report: Report,
    analyze_id: Option<u32>,
    stopping: bool,
    cancelled: bool,
    confirmations: Vec<Value>,
    deadline: Option<Instant>,
}
enum Purpose {
    Name(String),
    Version(String),
    Commands(String),
    Setup,
    Stop,
    Quit,
}
struct State {
    queue: Vec<Queued>,
    active: Option<Active>,
    pending: Option<(u32, Purpose)>,
    decoder: Decoder,
    sequence: u32,
    ordinal: u64,
    set_position: bool,
    name: String,
    version: String,
    writes: VecDeque<String>,
    outputs: VecDeque<Output>,
    closing: bool,
}

pub(crate) fn wrap(mut inner: Transport, cfg: &SessionConfig) -> Transport {
    let (outgoing, mut writes) = mpsc::channel::<String>(transport::CHANNEL_CAPACITY);
    let (received, incoming) = mpsc::channel(transport::CHANNEL_CAPACITY);
    let (exit, exited) = oneshot::channel();
    let (stop, mut stopping) = oneshot::channel();
    let shutdown_timeout = cfg.shutdown_timeout;
    let driver = Box::pin(async move {
        let mut state = State::new();
        let mut shutdown_deadline = None;
        let mut driver_done = false;
        loop {
            state.pump();
            let search_deadline = state.active.as_ref().and_then(|v| v.deadline);
            tokio::select! {
                _ = &mut inner.driver, if !driver_done => { driver_done = true; }
                report = &mut inner.exited, if driver_done => {
                    while let Ok(line) = inner.incoming.try_recv() { state.output(line); }
                    while let Some(output) = state.outputs.pop_front() { if received.send(output).await.is_err() { break; } }
                    if let Ok(mut report) = report { report.stderr_tail.clear(); let _ = exit.send(report); }
                    return;
                }
                permit = received.reserve(), if !state.outputs.is_empty() => {
                    if let Ok(permit) = permit { if let Some(line) = state.outputs.pop_front() { permit.send(line); } }
                    else { state.close(); shutdown_deadline.get_or_insert(Instant::now()+shutdown_timeout); }
                }
                permit = inner.outgoing.reserve(), if !state.writes.is_empty() && !driver_done => {
                    if let Ok(permit) = permit { if let Some(line) = state.writes.pop_front() { permit.send(line); } }
                    else { state.writes.clear(); }
                }
                line = inner.incoming.recv(), if !driver_done && state.outputs.len() < 256 => {
                    if let Some(line) = line { state.output(line); }
                }
                line = writes.recv(), if !state.closing => {
                    if let Some(line) = line { state.input(&line); }
                    else { state.close(); shutdown_deadline = Some(Instant::now()+shutdown_timeout); }
                }
                _ = &mut stopping, if !state.closing => { state.close(); shutdown_deadline = Some(Instant::now()+shutdown_timeout); }
                _ = deadline(search_deadline) => { state.stop_search(); }
                _ = deadline(shutdown_deadline) => {
                    shutdown_deadline = None;
                    if let Some(stop) = inner.stop.take() { let _ = stop.send(()); }
                }
            }
        }
    });
    Transport {
        outgoing,
        incoming,
        exited,
        stop: Some(stop),
        driver,
    }
}
async fn deadline(value: Option<Instant>) {
    match value {
        Some(v) => sleep_until(v).await,
        None => pending().await,
    }
}

impl State {
    fn new() -> Self {
        Self {
            queue: Vec::new(),
            active: None,
            pending: None,
            decoder: Decoder::default(),
            sequence: 0,
            ordinal: 0,
            set_position: false,
            name: String::new(),
            version: String::new(),
            writes: VecDeque::new(),
            outputs: VecDeque::new(),
            closing: false,
        }
    }
    fn emit(&mut self, value: Value) {
        self.outputs.push_back(Output::Stdout(value.to_string()));
    }
    fn error(&mut self, id: &str, error: BridgeError) {
        self.emit(json!({"id": id, "error": error.to_string(), "field": error.code()}));
    }
    fn command(&mut self, line: String, purpose: Option<Purpose>) -> u32 {
        self.sequence = self.sequence.wrapping_add(1);
        let id = self.sequence;
        self.writes.push_back(format!("{id} {line}"));
        if let Some(purpose) = purpose {
            self.pending = Some((id, purpose));
        }
        id
    }
    fn input(&mut self, line: &str) {
        match serde_json::from_str::<Request>(line) {
            Ok(Request::Analysis(query)) => match query::validate(&query) {
                Ok(turns) => {
                    self.ordinal = self.ordinal.wrapping_add(1);
                    self.queue.push(Queued {
                        query,
                        turns,
                        ordinal: self.ordinal,
                    });
                }
                Err(error) => self.error(&query.id, error),
            },
            Ok(Request::Action(Action::QueryVersion { id })) => {
                self.command("name".into(), Some(Purpose::Name(id)));
            }
            Ok(Request::Action(Action::Terminate { id, terminate_id })) => self.cancel(
                Some(&terminate_id),
                json!({"id":id, "action":"terminate", "terminateId":terminate_id}),
            ),
            Ok(Request::Action(Action::TerminateAll { id })) => {
                self.cancel(None, json!({"id":id,"action":"terminate_all"}))
            }
            Err(_) => self
                .outputs
                .push_back(Output::Fault(BridgeError::InvalidQuery.to_string())),
        }
    }
    fn cancel(&mut self, id: Option<&str>, confirmation: Value) {
        let mut retained = Vec::new();
        for queued in std::mem::take(&mut self.queue) {
            if id.is_none_or(|id| queued.query.id == id) {
                for turn in queued.turns {
                    self.no_results(&queued.query.id, turn);
                }
            } else {
                retained.push(queued);
            }
        }
        self.queue = retained;
        if let Some(active) = &mut self.active
            && id.is_none_or(|id| active.queued.query.id == id)
        {
            active.cancelled = true;
            active.confirmations.push(confirmation);
            // Stop also interrupts a command in the replay phase. Waiting for
            // that command's reply would make cancellation depend on a hung
            // remote engine completing work it has already been asked to stop.
            self.stop_search();
            return;
        }
        self.emit(confirmation);
    }
    fn no_results(&mut self, id: &str, turn: u32) {
        self.emit(json!({"id":id,"turnNumber":turn,"isDuringSearch":false,"noResults":true}));
    }
    fn pump(&mut self) {
        if self.closing || self.pending.is_some() {
            return;
        }
        if self.active.is_none() {
            let next = self
                .queue
                .iter()
                .enumerate()
                .max_by_key(|(_, q)| (q.query.priority.unwrap_or(0), std::cmp::Reverse(q.ordinal)))
                .map(|(index, _)| index);
            let Some(index) = next else {
                return;
            };
            let mut queued = self.queue.remove(index);
            let Some(turn) = queued.turns.pop_front() else {
                return;
            };
            match query::setup(&queued.query, turn, self.set_position) {
                Ok(setup) => {
                    self.active = Some(Active {
                        queued,
                        turn,
                        setup,
                        report: Report::default(),
                        analyze_id: None,
                        stopping: false,
                        cancelled: false,
                        confirmations: Vec::new(),
                        deadline: None,
                    })
                }
                Err(error) => {
                    self.error(&queued.query.id, error);
                    return;
                }
            }
        }
        let Some(active) = &mut self.active else {
            return;
        };
        if active.cancelled {
            self.stop_search();
            return;
        }
        if active.analyze_id.is_some() {
            return;
        }
        if let Some(command) = active.setup.pop_front() {
            self.command(command, Some(Purpose::Setup));
        } else {
            let line = query::analyze(&active.queued.query, active.turn);
            let time = query::max_time(&active.queued.query).ok().flatten();
            let id = self.command(line, None);
            if let Some(active) = &mut self.active {
                active.analyze_id = Some(id);
                active.deadline = time.map(|duration| Instant::now() + duration);
            }
        }
    }
    fn stop_search(&mut self) {
        if let Some(active) = &mut self.active {
            if active.stopping {
                return;
            }
            active.stopping = true;
            active.deadline = None;
            self.command("stop".into(), Some(Purpose::Stop));
        }
    }
    fn output(&mut self, output: Output) {
        match output {
            Output::Stdout(line) => {
                if line.starts_with("info ")
                    || line.starts_with("rootInfo ")
                    || line.starts_with("ownership ")
                    || line.starts_with("ownershipStdev ")
                {
                    self.analysis(&line);
                } else {
                    match self.decoder.push_line(&line) {
                        Ok(Some(response)) => self.reply(response),
                        Ok(None) => {}
                        Err(_) => self.outputs.push_back(Output::Fault(
                            "zhizi bridge: malformed GTP response".into(),
                        )),
                    }
                }
            }
            // Remote stderr may contain secrets/account metadata. It is only a
            // heartbeat here; do not retain arbitrary provider text in logs.
            Output::Stderr(_) => self
                .outputs
                .push_back(Output::Stderr("zhizi GTP diagnostic received".into())),
            output => self.outputs.push_back(output),
        }
    }
    fn analysis(&mut self, line: &str) {
        let Some(active) = &mut self.active else {
            return;
        };
        if active.analyze_id.is_none() {
            return;
        }
        let parsed = gtp::kata_analyze::parse(line).map_err(|_| BridgeError::MalformedReport);
        let result = parsed.and_then(|parsed| {
            report::merge(&mut active.report, parsed);
            report::response(&active.queued.query, active.turn, &active.report, true)
        });
        let stop = active
            .report
            .root
            .as_ref()
            .is_some_and(|root| root.visits >= active.queued.query.max_visits.unwrap_or(1000));
        match result {
            Ok(Some(response)) => self.emit(response),
            Ok(None) => {}
            Err(error) => {
                self.fail_active(error);
                return;
            }
        }
        if stop {
            self.stop_search();
        }
    }
    fn reply(&mut self, response: Response) {
        let Some((id, _)) = self.pending.as_ref() else {
            if !response.success
                && self
                    .active
                    .as_ref()
                    .is_some_and(|a| a.analyze_id == response.id)
            {
                self.fail_active(BridgeError::CommandRejected);
            }
            return;
        };
        if response.id != Some(*id) {
            return;
        }
        let Some((_, purpose)) = self.pending.take() else {
            return;
        };
        if !response.success {
            if matches!(purpose, Purpose::Setup | Purpose::Stop) {
                self.fail_active(BridgeError::CommandRejected);
            } else {
                self.outputs
                    .push_back(Output::Fault("zhizi bridge: handshake rejected".into()));
            }
            return;
        }
        match purpose {
            Purpose::Name(id) => {
                self.name = response.body;
                self.command("version".into(), Some(Purpose::Version(id)));
            }
            Purpose::Version(id) => {
                self.version = response.body;
                self.command("list_commands".into(), Some(Purpose::Commands(id)));
            }
            Purpose::Commands(id) => {
                self.set_position = response
                    .body
                    .lines()
                    .any(|line| line.trim() == "set_position");
                self.emit(json!({"id":id,"action":"query_version","version":self.version,"git_hash":"zhizi-gtp-bridge"}));
            }
            Purpose::Setup => {}
            Purpose::Stop => self.finish(),
            Purpose::Quit => {}
        }
    }
    fn finish(&mut self) {
        let Some(mut active) = self.active.take() else {
            return;
        };
        match report::response(&active.queued.query, active.turn, &active.report, false) {
            Ok(Some(response)) => self.emit(response),
            Ok(None) => self.no_results(&active.queued.query.id, active.turn),
            Err(error) => self.error(&active.queued.query.id, error),
        }
        if active.cancelled {
            for turn in active.queued.turns {
                self.no_results(&active.queued.query.id, turn);
            }
            for value in active.confirmations {
                self.emit(value);
            }
        } else if let Some(turn) = active.queued.turns.pop_front() {
            match query::setup(&active.queued.query, turn, self.set_position) {
                Ok(setup) => {
                    active.turn = turn;
                    active.setup = setup;
                    active.report = Report::default();
                    active.analyze_id = None;
                    active.stopping = false;
                    active.deadline = None;
                    self.active = Some(active);
                }
                Err(error) => self.error(&active.queued.query.id, error),
            }
        }
    }
    fn fail_active(&mut self, error: BridgeError) {
        if let Some(active) = self.active.take() {
            self.error(&active.queued.query.id, error);
            for value in active.confirmations {
                self.emit(value);
            }
            // Stop before any following board-replay command; subsequent stale
            // reports are discarded because the failed active query is gone.
            self.command("stop".into(), Some(Purpose::Stop));
        }
    }
    fn close(&mut self) {
        self.closing = true;
        self.queue.clear();
        self.active = None;
        self.writes.clear();
        self.command("quit".into(), Some(Purpose::Quit));
    }
}

#[cfg(test)]
mod tests;
