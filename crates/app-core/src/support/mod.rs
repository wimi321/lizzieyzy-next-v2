//! Bounded local support logs and explicit, sanitized diagnostic exports.
//! A single Tokio task owns the log files and the temporary verbosity deadline.

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use ts_rs::TS;

use crate::{EngineDiagnostics, Settings};

mod bundle;
mod layer;
mod paths;
mod rolling;
mod sanitize;
#[cfg(test)]
mod tests;

pub use layer::SupportLogLayer;

const FULL_LOG_DURATION: Duration = Duration::from_secs(30 * 60);
const QUEUE_CAPACITY: usize = 512;
const MAX_RECORD_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FullLogStatus {
    #[ts(as = "Option<bool>", optional)]
    pub enabled: bool,
    #[ts(as = "Option<f64>", optional)]
    pub remaining_seconds: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum SupportError {
    #[error("support logging has stopped")]
    Closed,
    #[error("support file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("diagnostic JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("diagnostic archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("support task error: {0}")]
    Task(String),
    #[error("cannot initialize tracing: {0}")]
    Tracing(String),
    #[error("only existing paths inside the application data directory may be revealed")]
    RevealOutsideDataDirectory,
}

#[derive(Clone)]
pub struct SupportHandle {
    commands: mpsc::Sender<Message>,
    verbose: watch::Receiver<bool>,
}

struct BundleInput {
    settings: Settings,
    diagnostics: EngineDiagnostics,
    version: String,
}

enum Message {
    Record {
        level: tracing::Level,
        at: Instant,
        text: String,
    },
    Status(oneshot::Sender<FullLogStatus>),
    Directory(oneshot::Sender<PathBuf>),
    SetVerbose(bool, oneshot::Sender<FullLogStatus>),
    Export(
        Box<BundleInput>,
        oneshot::Sender<Result<PathBuf, SupportError>>,
    ),
    Flush(oneshot::Sender<Result<(), SupportError>>),
    Shutdown(oneshot::Sender<Result<(), SupportError>>),
}

/// Files are created lazily, so merely loading an application does not change its
/// data directory. This must be called inside the application's Tokio runtime.
pub async fn spawn(data_directory: PathBuf) -> Result<SupportHandle, SupportError> {
    Ok(spawn_with_limits(
        std::path::absolute(data_directory)?,
        rolling::Limits::default(),
    ))
}

fn spawn_with_limits(data_directory: PathBuf, limits: rolling::Limits) -> SupportHandle {
    let (commands, receiver) = mpsc::channel(QUEUE_CAPACITY);
    let (verbose_tx, verbose) = watch::channel(false);
    tokio::spawn(run(data_directory, limits, receiver, verbose_tx));
    SupportHandle { commands, verbose }
}

impl SupportHandle {
    /// The configured logging location, without creating or probing its files.
    pub async fn log_directory(&self) -> Result<PathBuf, SupportError> {
        Ok(std::path::absolute(self.data_directory().await?)?.join("logs"))
    }

    /// Resolve both sides before authorization. The returned path is suitable
    /// for the platform reveal adapter; the caller must not reuse the raw input.
    pub async fn resolve_reveal_path(&self, path: PathBuf) -> Result<PathBuf, SupportError> {
        paths::resolve(&self.data_directory().await?, &path).await
    }

    async fn data_directory(&self) -> Result<PathBuf, SupportError> {
        let (reply, receive) = oneshot::channel();
        self.send(Message::Directory(reply)).await?;
        receive.await.map_err(|_| SupportError::Closed)
    }

    pub fn layer(&self) -> SupportLogLayer {
        SupportLogLayer::new(self.commands.clone(), self.verbose.clone())
    }

    /// Only the desktop entry point installs a process-wide subscriber. Tests
    /// can use `layer()` with a scoped dispatcher without changing global state.
    pub fn install_global(&self) -> Result<(), SupportError> {
        tracing_subscriber::registry()
            .with(self.layer())
            .try_init()
            .map_err(|error| SupportError::Tracing(error.to_string()))
    }

    pub async fn full_log_status(&self) -> Result<FullLogStatus, SupportError> {
        let (reply, receive) = oneshot::channel();
        self.send(Message::Status(reply)).await?;
        receive.await.map_err(|_| SupportError::Closed)
    }

    pub async fn start_full_log(&self) -> Result<FullLogStatus, SupportError> {
        self.set_verbose(true).await
    }

    pub async fn stop_full_log(&self) -> Result<FullLogStatus, SupportError> {
        self.set_verbose(false).await
    }

    async fn set_verbose(&self, enabled: bool) -> Result<FullLogStatus, SupportError> {
        let (reply, receive) = oneshot::channel();
        self.send(Message::SetVerbose(enabled, reply)).await?;
        receive.await.map_err(|_| SupportError::Closed)
    }

    pub async fn export_bundle(
        &self,
        settings: Settings,
        diagnostics: EngineDiagnostics,
        version: String,
    ) -> Result<PathBuf, SupportError> {
        let (reply, receive) = oneshot::channel();
        self.send(Message::Export(
            Box::new(BundleInput {
                settings,
                diagnostics,
                version,
            }),
            reply,
        ))
        .await?;
        receive.await.map_err(|_| SupportError::Closed)?
    }

    /// A barrier for accepted log records, also used before application exit.
    pub async fn flush(&self) -> Result<(), SupportError> {
        let (reply, receive) = oneshot::channel();
        self.send(Message::Flush(reply)).await?;
        receive.await.map_err(|_| SupportError::Closed)?
    }

    pub async fn shutdown(&self) -> Result<(), SupportError> {
        let (reply, receive) = oneshot::channel();
        self.send(Message::Shutdown(reply)).await?;
        receive.await.map_err(|_| SupportError::Closed)?
    }

    async fn send(&self, message: Message) -> Result<(), SupportError> {
        self.commands
            .send(message)
            .await
            .map_err(|_| SupportError::Closed)
    }
}

fn status(deadline: Option<Instant>) -> FullLogStatus {
    let remaining = deadline.map(|at| at.saturating_duration_since(Instant::now()));
    let remaining_seconds = remaining.map_or(0, |duration| {
        duration
            .as_secs()
            .saturating_add(u64::from(duration.subsec_nanos() > 0))
    });
    FullLogStatus {
        enabled: remaining_seconds > 0,
        remaining_seconds,
    }
}

async fn run(
    directory: PathBuf,
    limits: rolling::Limits,
    mut messages: mpsc::Receiver<Message>,
    verbose: watch::Sender<bool>,
) {
    let mut logs = rolling::Logs::new(directory.join("logs"), limits);
    let mut deadline = None;
    // Keep the last interval after its timer expires: queued records are judged
    // by their capture time, not the speed of the disk or actor scheduling.
    let mut verbose_window = None;
    let mut write_error = None;
    loop {
        let message = tokio::select! {
            biased;
            () = async {
                match deadline {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            } => {
                deadline = None;
                verbose.send_replace(false);
                continue;
            }
            message = messages.recv() => message,
        };
        let Some(message) = message else { break };
        match message {
            Message::Record { level, at, text } => {
                if level <= tracing::Level::INFO
                    || verbose_window.is_some_and(|(start, end)| start <= at && at < end)
                {
                    let safe = sanitize::text(&text);
                    if let Err(error) = logs.append(safe.as_bytes()).await {
                        write_error = Some(error.to_string());
                    }
                }
            }
            Message::Status(reply) => {
                let _ = reply.send(status(deadline));
            }
            Message::Directory(reply) => {
                let _ = reply.send(directory.clone());
            }
            Message::SetVerbose(enabled, reply) => {
                let now = Instant::now();
                deadline = enabled.then_some(now + FULL_LOG_DURATION);
                if let Some(end) = deadline {
                    verbose_window = Some((now, end));
                } else if let Some((_, end)) = &mut verbose_window {
                    *end = (*end).min(now);
                }
                verbose.send_replace(enabled);
                let _ = reply.send(status(deadline));
            }
            Message::Export(input, reply) => {
                let result = export(&mut logs, &directory, *input, write_error.as_deref()).await;
                let _ = reply.send(result);
            }
            Message::Flush(reply) => {
                let _ = reply.send(flush(&mut logs, write_error.as_deref()).await);
            }
            Message::Shutdown(reply) => {
                verbose.send_replace(false);
                let _ = reply.send(flush(&mut logs, write_error.as_deref()).await);
                return;
            }
        }
    }
    verbose.send_replace(false);
    let _ = logs.flush().await;
}

async fn flush(logs: &mut rolling::Logs, error: Option<&str>) -> Result<(), SupportError> {
    logs.flush().await?;
    match error {
        Some(error) => Err(SupportError::Io(std::io::Error::other(error))),
        None => Ok(()),
    }
}

async fn export(
    logs: &mut rolling::Logs,
    directory: &std::path::Path,
    input: BundleInput,
    error: Option<&str>,
) -> Result<PathBuf, SupportError> {
    flush(logs, error).await?;
    let entries = logs.snapshot().await?;
    let destination = directory.join("diagnostics");
    tokio::task::spawn_blocking(move || bundle::write(destination, input, entries))
        .await
        .map_err(|error| SupportError::Task(error.to_string()))?
}
