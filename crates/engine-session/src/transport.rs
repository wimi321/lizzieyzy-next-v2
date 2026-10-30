//! Local-process I/O. The session actor polls the driver directly, so the child
//! is never owned by a detached task and shutdown can wait for its exit status.

use std::collections::VecDeque;
use std::future::{Future, pending};
use std::io;
use std::pin::Pin;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, sleep_until, timeout};

use crate::{SessionConfig, TransportSpec};

pub(crate) const CHANNEL_CAPACITY: usize = 64;
const STDERR_LINES: usize = 200;

pub(crate) enum Output {
    Stdout(String),
    Stderr(String),
    Fault(String),
}

pub(crate) struct Exit {
    pub status: Result<ExitStatus, String>,
    pub stderr_tail: Vec<String>,
}

pub(crate) struct Transport {
    pub outgoing: mpsc::Sender<String>,
    pub incoming: mpsc::Receiver<Output>,
    pub exited: oneshot::Receiver<Exit>,
    pub stop: Option<oneshot::Sender<()>>,
    pub driver: Pin<Box<dyn Future<Output = ()> + Send>>,
}

type Writer = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

pub(crate) fn start(spec: &TransportSpec, cfg: &SessionConfig) -> io::Result<Transport> {
    let TransportSpec::LocalProcess {
        program,
        args,
        cwd,
        env,
    } = spec;
    let mut command = Command::new(program);
    command
        .args(args)
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command.spawn()?;
    let input = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("child stdin pipe was not created"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("child stdout pipe was not created"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("child stderr pipe was not created"))?;
    let (outgoing, writes) = mpsc::channel(CHANNEL_CAPACITY);
    let (received, incoming) = mpsc::channel(CHANNEL_CAPACITY);
    let (exit, exited) = oneshot::channel();
    let (stop, stopping) = oneshot::channel();
    let writer: Writer = Box::pin(write_lines(input, writes, cfg.write_timeout));
    let shutdown_timeout = cfg.shutdown_timeout;
    let driver = Box::pin(async move {
        let report = drive(
            child,
            writer,
            BufReader::new(stdout).lines(),
            BufReader::new(stderr).lines(),
            received,
            stopping,
            shutdown_timeout,
        )
        .await;
        let _ = exit.send(report);
    });
    Ok(Transport {
        outgoing,
        incoming,
        exited,
        stop: Some(stop),
        driver,
    })
}

async fn write_lines(
    mut input: ChildStdin,
    mut writes: mpsc::Receiver<String>,
    write_timeout: Duration,
) -> Result<(), String> {
    while let Some(mut line) = writes.recv().await {
        line.push('\n');
        timeout(write_timeout, input.write_all(line.as_bytes()))
            .await
            .map_err(|_| "child stdin write timed out".to_owned())?
            .map_err(|error| format!("child stdin write failed: {error}"))?;
    }
    Ok(())
}

async fn poll_writer(writer: &mut Option<Writer>) -> Result<(), String> {
    match writer {
        Some(writer) => writer.await,
        None => pending().await,
    }
}

async fn shutdown_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => pending().await,
    }
}

async fn drive(
    mut child: Child,
    writer: Writer,
    mut stdout: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    mut stderr: tokio::io::Lines<BufReader<tokio::process::ChildStderr>>,
    received: mpsc::Sender<Output>,
    mut stop: oneshot::Receiver<()>,
    shutdown_timeout: Duration,
) -> Exit {
    let mut writer = Some(writer);
    let mut stopped = false;
    let mut deadline = None;
    let mut status = None;
    let mut stdout_open = true;
    let mut stderr_open = true;
    let mut stderr_tail = VecDeque::with_capacity(STDERR_LINES);

    while status.is_none() || stdout_open || stderr_open {
        tokio::select! {
            result = child.wait(), if status.is_none() => {
                status = Some(result.map_err(|error| format!("child wait failed: {error}")));
                // Closing stdin also discards queued requests for a dead process.
                writer = None;
                deadline = None;
            }
            _ = &mut stop, if !stopped => {
                stopped = true;
                writer = None;
                if status.is_none() {
                    deadline = Some(Instant::now() + shutdown_timeout);
                }
            }
            result = poll_writer(&mut writer) => {
                writer = None;
                stopped = true;
                deadline = Some(Instant::now() + shutdown_timeout);
                if let Err(error) = result {
                    let _ = received.send(Output::Fault(error)).await;
                }
            }
            _ = shutdown_deadline(deadline), if status.is_none() => {
                deadline = None;
                if let Err(error) = child.start_kill() {
                    let _ = received.send(Output::Fault(format!("child kill failed: {error}"))).await;
                }
                // Continue polling wait: a successful kill request alone is not
                // evidence that the child has exited or been reaped.
            }
            line = stdout.next_line(), if stdout_open => {
                match line {
                    Ok(Some(line)) => { let _ = received.send(Output::Stdout(line)).await; }
                    Ok(None) => stdout_open = false,
                    Err(error) => {
                        stdout_open = false;
                        let _ = received.send(Output::Fault(format!("child stdout read failed: {error}"))).await;
                    }
                }
            }
            line = stderr.next_line(), if stderr_open => {
                match line {
                    Ok(Some(line)) => {
                        if stderr_tail.len() == STDERR_LINES {
                            stderr_tail.pop_front();
                        }
                        stderr_tail.push_back(line.clone());
                        let _ = received.send(Output::Stderr(line)).await;
                    }
                    Ok(None) => stderr_open = false,
                    Err(error) => {
                        stderr_open = false;
                        let _ = received.send(Output::Fault(format!("child stderr read failed: {error}"))).await;
                    }
                }
            }
        }
    }
    Exit {
        status: status.unwrap_or_else(|| Err("child exit status unavailable".into())),
        stderr_tail: stderr_tail.into(),
    }
}

#[cfg(test)]
mod tests;
