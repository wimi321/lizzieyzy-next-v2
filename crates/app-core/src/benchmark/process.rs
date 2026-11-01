//! The app actor owns this one-shot process, including both output pipes.

use super::BenchmarkError;
use crate::{BenchmarkMeasurement, BenchmarkProgress, EngineProfile};
use engine_session::TransportSpec;
use katago_protocol::benchmark::{BenchmarkMetrics, BenchmarkParser};
use std::{ffi::OsString, process::ExitStatus, process::Stdio, time::Duration};
use tokio::{
    io::AsyncReadExt,
    process::{Child, ChildStderr, ChildStdout, Command},
    time::Instant,
};

// A safety ceiling, not an expected duration. Keep headroom for slow hardware
// and first-use initialization even with the shorter four-position quick run.
const RUN_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const TAIL_BYTES: usize = 8192;

pub(crate) struct Process {
    child: Child,
    stdout: ChildStdout,
    stderr: ChildStderr,
    stdout_open: bool,
    stderr_open: bool,
    exit: Option<ExitStatus>,
    parser: BenchmarkParser,
    stdout_tail: Vec<u8>,
    stderr_tail: Vec<u8>,
    deadline: Instant,
    explicit_batch: Option<String>,
    explicit_server: Option<String>,
}

pub(crate) enum Update {
    Progress {
        progress: BenchmarkProgress,
        measurements: Vec<BenchmarkMeasurement>,
    },
    Finished {
        metrics: BenchmarkMetrics,
        measurements: Vec<BenchmarkMeasurement>,
    },
}

impl Process {
    pub async fn spawn(profile: &EngineProfile) -> Result<Self, BenchmarkError> {
        let TransportSpec::LocalProcess {
            program,
            args,
            cwd,
            env,
        } = benchmark_transport(profile)?;
        let (explicit_batch, explicit_server) = explicit_gpu_overrides(&args);
        let mut command = Command::new(program);
        command
            .args(args)
            .envs(env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        let mut child = command.spawn().map_err(failed)?;
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill().await;
            return Err(failed("missing benchmark stdout pipe"));
        };
        let Some(stderr) = child.stderr.take() else {
            let _ = child.kill().await;
            return Err(failed("missing benchmark stderr pipe"));
        };
        Ok(Self {
            child,
            stdout,
            stderr,
            stdout_open: true,
            stderr_open: true,
            exit: None,
            parser: BenchmarkParser::default(),
            stdout_tail: Vec::new(),
            stderr_tail: Vec::new(),
            deadline: Instant::now() + RUN_TIMEOUT,
            explicit_batch,
            explicit_server,
        })
    }

    /// All partially read bytes live in the parser. Losing a select race to an
    /// app command cannot discard progress, UTF-8 fragments, or an exit status.
    pub async fn next(&mut self) -> Result<Update, BenchmarkError> {
        let mut stdout = [0; 4096];
        let mut stderr = [0; 4096];
        loop {
            if let Some(exit) = self.exit
                && !self.stdout_open
                && !self.stderr_open
            {
                if !exit.success() {
                    return Err(self.diagnostic(format!("benchmark exited with {exit}")));
                }
                let metrics = self
                    .parser
                    .finish_with_overrides(
                        self.explicit_batch.as_deref(),
                        self.explicit_server.as_deref(),
                    )
                    .map_err(|error| self.diagnostic(error))?;
                return Ok(Update::Finished {
                    metrics,
                    measurements: self.parser.metrics().iter().map(measurement).collect(),
                });
            }
            tokio::select! {
                result = self.stdout.read(&mut stdout), if self.stdout_open => {
                    let count = result.map_err(|error| self.diagnostic(error))?;
                    self.stdout_open = count != 0;
                    append_tail(&mut self.stdout_tail, &stdout[..count]);
                    let progress = self.parser.push(&stdout[..count])
                        .map_err(|error| self.diagnostic(error))?;
                    if let Some(progress) = progress.into_iter().last() {
                        return Ok(Update::Progress {
                            progress: BenchmarkProgress {
                                threads: progress.threads,
                                completed_positions: progress.completed_positions,
                                total_positions: progress.total_positions,
                                visits_per_second: progress.visits_per_second,
                                nn_evals_per_second: progress.nn_evals_per_second,
                            },
                            measurements: self.parser.metrics().iter().map(measurement).collect(),
                        });
                    }
                }
                result = self.stderr.read(&mut stderr), if self.stderr_open => {
                    let count = result.map_err(|error| self.diagnostic(error))?;
                    self.stderr_open = count != 0;
                    append_tail(&mut self.stderr_tail, &stderr[..count]);
                }
                result = self.child.wait(), if self.exit.is_none() => {
                    self.exit = Some(result.map_err(|error| self.diagnostic(error))?);
                }
                _ = tokio::time::sleep_until(self.deadline) => {
                    return Err(self.diagnostic("benchmark exceeded the 60 minute limit"));
                }
            }
        }
    }

    /// Completion of this method confirms the child has been reaped. No
    /// analysis process may resume before this finishes, including on errors.
    pub async fn stop(&mut self) -> Result<(), BenchmarkError> {
        if self.exit.is_none() {
            if let Some(exit) = self.child.try_wait().map_err(failed)? {
                self.exit = Some(exit);
                return Ok(());
            }
            match self.child.start_kill() {
                Ok(()) => self.exit = Some(self.child.wait().await.map_err(failed)?),
                Err(error) => {
                    // The process can exit between try_wait and start_kill.
                    // Still reap it; a real kill failure must not hang callers.
                    match tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await {
                        Ok(Ok(exit)) => self.exit = Some(exit),
                        _ => return Err(failed(format!("cannot stop benchmark: {error}"))),
                    }
                }
            }
        }
        Ok(())
    }

    fn diagnostic(&self, reason: impl std::fmt::Display) -> BenchmarkError {
        let stderr = String::from_utf8_lossy(&self.stderr_tail);
        let stdout = String::from_utf8_lossy(&self.stdout_tail);
        BenchmarkError::Failed(format!(
            "{reason}\nstdout: {}\nstderr: {}",
            stdout.trim(),
            stderr.trim()
        ))
    }
}

fn measurement(metric: &BenchmarkMetrics) -> BenchmarkMeasurement {
    BenchmarkMeasurement {
        threads: metric.threads,
        completed_positions: metric.completed_positions,
        total_positions: metric.total_positions,
        visits_per_second: metric.visits_per_second,
        nn_evals_per_second: metric.nn_evals_per_second,
    }
}

/// The transport has already merged and validated one override list. Mirror
/// KataGo's last-value-wins rule to identify which recommendations can apply.
fn explicit_gpu_overrides(args: &[OsString]) -> (Option<String>, Option<String>) {
    let mut batch = None;
    let mut server = None;
    if let Some(index) = args.iter().position(|arg| arg == "-override-config")
        && let Some(value) = args.get(index + 1).and_then(|value| value.to_str())
    {
        for entry in value.split(',') {
            if let Some((key, value)) = entry.split_once('=') {
                if key.trim().eq_ignore_ascii_case("nnMaxBatchSize") {
                    batch = Some(value.trim().to_owned());
                } else if key
                    .trim()
                    .eq_ignore_ascii_case("numNNServerThreadsPerModel")
                {
                    server = Some(value.trim().to_owned());
                }
            }
        }
    }
    (batch, server)
}

fn append_tail(tail: &mut Vec<u8>, bytes: &[u8]) {
    tail.extend_from_slice(bytes);
    if tail.len() > TAIL_BYTES {
        tail.drain(..tail.len() - TAIL_BYTES);
    }
}

fn failed(error: impl std::fmt::Display) -> BenchmarkError {
    BenchmarkError::Failed(error.to_string())
}

fn benchmark_transport(profile: &EngineProfile) -> Result<TransportSpec, BenchmarkError> {
    let TransportSpec::LocalProcess {
        program,
        args,
        cwd,
        env,
    } = profile.transport().map_err(failed)?;
    let mut benchmark_args = vec![OsString::from("benchmark")];
    let mut overrides = Vec::new();
    let mut args = args.into_iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-override-config" {
            let value = args
                .next()
                .ok_or_else(|| failed("missing override value"))?;
            let value = value
                .to_str()
                .ok_or_else(|| failed("override configuration is not UTF-8"))?;
            overrides.extend(value.split(',').filter_map(|entry| {
                let (key, _) = entry.split_once('=')?;
                let managed = [
                    "numSearchThreads",
                    "numAnalysisThreads",
                    "numSearchThreadsPerAnalysisThread",
                    "logToStderr",
                    "logAllGTPCommunication",
                    "logSearchInfo",
                ];
                (!managed
                    .iter()
                    .any(|name| key.trim().eq_ignore_ascii_case(name)))
                .then(|| entry.to_owned())
            }));
        } else {
            benchmark_args.push(arg);
        }
    }
    // benchmark reads GTP search parameters. Empty values remove the analysis
    // aliases even when they originate in an included user configuration.
    overrides.extend([
        "numAnalysisThreads=".into(),
        "numSearchThreadsPerAnalysisThread=".into(),
        "numSearchThreads=1".into(),
        "logToStderr=false".into(),
        "logAllGTPCommunication=false".into(),
        "logSearchInfo=false".into(),
    ]);
    benchmark_args.extend(
        [
            "-s",
            "-n",
            "4",
            "-v",
            "400",
            "-time",
            "5",
            "-override-config",
        ]
        .map(OsString::from),
    );
    benchmark_args.push(overrides.join(",").into());
    Ok(TransportSpec::LocalProcess {
        program,
        args: benchmark_args,
        cwd,
        env,
    })
}

#[cfg(test)]
mod tests;
