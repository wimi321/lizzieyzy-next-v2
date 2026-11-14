//! Independent GTP actor: a single command in flight, with no automatic restart.

mod actor;

use crate::TransportSpec;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot, watch};

#[derive(Clone, Debug)]
pub struct GtpSessionConfig {
    pub startup_timeout: Duration,
    pub response_timeout: Duration,
    pub write_timeout: Duration,
    pub shutdown_timeout: Duration,
    pub queue_capacity: usize,
    pub init_commands: Vec<String>,
}

impl Default for GtpSessionConfig {
    fn default() -> Self {
        Self {
            startup_timeout: Duration::from_secs(120),
            response_timeout: Duration::from_secs(120),
            write_timeout: Duration::from_secs(5),
            shutdown_timeout: Duration::from_secs(2),
            queue_capacity: 64,
            init_commands: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GtpInfo {
    pub protocol_version: String,
    pub name: String,
    pub version: String,
    pub commands: Vec<String>,
}

impl GtpInfo {
    pub fn supports(&self, command: &str) -> bool {
        self.commands.iter().any(|name| name == command)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GtpSessionError {
    #[error("GTP session is closed")]
    Closed,
    #[error("GTP command queue is full")]
    Capacity,
    #[error("invalid GTP session configuration: {0}")]
    InvalidConfig(String),
    #[error("GTP process failed: {0}")]
    Process(String),
    #[error("GTP command timed out: {command}")]
    Timeout { command: String },
    #[error("GTP protocol error: {0}")]
    Protocol(String),
    #[error("GTP command {command} rejected: {message}")]
    Rejected { command: String, message: String },
    #[error("GTP startup command {command} failed: {message}")]
    Startup { command: String, message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GtpStatus {
    Starting,
    Ready(GtpInfo),
    Failed(GtpSessionError),
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GtpDirection {
    Sent,
    Received,
    Stderr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GtpLogLine {
    pub sequence: u64,
    pub direction: GtpDirection,
    pub text: String,
}

/// Reply to an already enqueued command. Dropping it does not cancel the wire command.
pub struct GtpReply {
    received: oneshot::Receiver<Result<::gtp::Response, GtpSessionError>>,
}

impl GtpReply {
    pub async fn recv(self) -> Result<::gtp::Response, GtpSessionError> {
        self.received.await.map_err(|_| GtpSessionError::Closed)?
    }
}

#[derive(Clone)]
pub struct GtpHandle {
    commands: mpsc::Sender<actor::Message>,
    status: watch::Receiver<GtpStatus>,
    lines: broadcast::Sender<GtpLogLine>,
    final_history: watch::Receiver<Vec<GtpLogLine>>,
}

pub fn spawn(transport: TransportSpec, config: GtpSessionConfig) -> GtpHandle {
    let (commands, receiver) = mpsc::channel(config.queue_capacity.clamp(1, 1024));
    let (status, status_rx) = watch::channel(GtpStatus::Starting);
    let (lines, _) = broadcast::channel(2000);
    let (final_history, history_rx) = watch::channel(Vec::new());
    tokio::spawn(actor::run(
        transport,
        config,
        receiver,
        status,
        lines.clone(),
        final_history,
    ));
    GtpHandle {
        commands,
        status: status_rx,
        lines,
        final_history: history_rx,
    }
}

impl GtpHandle {
    pub fn status(&self) -> watch::Receiver<GtpStatus> {
        self.status.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<GtpLogLine> {
        self.lines.subscribe()
    }

    pub async fn ready(&self) -> Result<GtpInfo, GtpSessionError> {
        let mut status = self.status.clone();
        loop {
            match status.borrow_and_update().clone() {
                GtpStatus::Starting => {}
                GtpStatus::Ready(info) => return Ok(info),
                GtpStatus::Failed(error) => return Err(error),
                GtpStatus::Stopped => return Err(GtpSessionError::Closed),
            }
            status
                .changed()
                .await
                .map_err(|_| GtpSessionError::Closed)?;
        }
    }

    /// Enqueue synchronously so an application actor can preserve command ordering
    /// without waiting for a potentially long-running response.
    pub fn enqueue_command(&self, text: impl Into<String>) -> Result<GtpReply, GtpSessionError> {
        self.enqueue(text.into(), None)
    }

    fn enqueue(
        &self,
        text: String,
        timeout: Option<Duration>,
    ) -> Result<GtpReply, GtpSessionError> {
        if timeout.is_some_and(|duration| {
            duration.is_zero() || tokio::time::Instant::now().checked_add(duration).is_none()
        }) {
            return Err(GtpSessionError::InvalidConfig(
                "command timeout must be positive and representable".into(),
            ));
        }
        ::gtp::Command::parse(&text)
            .map_err(|error| GtpSessionError::Protocol(error.to_string()))?;
        let (reply, received) = oneshot::channel();
        self.commands
            .try_send(actor::Message::Execute(text, timeout, reply))
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => GtpSessionError::Capacity,
                mpsc::error::TrySendError::Closed(_) => GtpSessionError::Closed,
            })?;
        Ok(GtpReply { received })
    }

    pub async fn command(
        &self,
        text: impl Into<String>,
    ) -> Result<::gtp::Response, GtpSessionError> {
        self.enqueue_command(text)?.recv().await
    }

    /// Override only this command response deadline, measured once it is sent.
    /// Handshake deadlines and other queued commands retain their own budgets.
    pub async fn command_with_timeout(
        &self,
        text: impl Into<String>,
        timeout: Duration,
    ) -> Result<::gtp::Response, GtpSessionError> {
        self.enqueue(text.into(), Some(timeout))?.recv().await
    }

    pub async fn console(&self) -> Result<Vec<GtpLogLine>, GtpSessionError> {
        let (reply, received) = oneshot::channel();
        if self
            .commands
            .send(actor::Message::Console(reply))
            .await
            .is_err()
        {
            return Ok(self.final_history.borrow().clone());
        }
        Ok(received
            .await
            .unwrap_or_else(|_| self.final_history.borrow().clone()))
    }

    /// Cancel pending commands and wait for the process to be reaped.
    pub async fn shutdown(self) -> Result<(), GtpSessionError> {
        if *self.status.borrow() == GtpStatus::Stopped {
            return Ok(());
        }
        let (reply, received) = oneshot::channel();
        self.commands
            .send(actor::Message::Shutdown(reply))
            .await
            .map_err(|_| GtpSessionError::Closed)?;
        received.await.map_err(|_| GtpSessionError::Closed)?
    }
}
