//! Actor-owned KataGo engine sessions.

mod actor;
mod delivery;
mod request;
mod transport;

pub use delivery::QuerySubscription;
pub use request::AnalysisRequest;

use katago_protocol::AnalysisResponse;
use std::{collections::BTreeMap, ffi::OsString, path::PathBuf, time::Duration};
use tokio::sync::{mpsc, oneshot, watch};

pub type QueryId = String;

/// Local executable and arguments are used verbatim, without a shell.
#[derive(Clone, Debug)]
pub enum TransportSpec {
    LocalProcess {
        program: PathBuf,
        args: Vec<OsString>,
        cwd: Option<PathBuf>,
        env: BTreeMap<OsString, OsString>,
    },
}

#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub startup_timeout: Duration,
    pub response_timeout: Duration,
    pub restart_backoff: Duration,
    pub max_restart_backoff: Duration,
    pub shutdown_timeout: Duration,
    pub write_timeout: Duration,
    /// Total restarts over this session's lifetime; Ready does not reset it.
    pub max_restarts: u32,
    /// Tolerated consecutive malformed stdout lines (zero fails on the first).
    pub max_consecutive_bad_lines: u32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            startup_timeout: Duration::from_secs(120),
            response_timeout: Duration::from_secs(30),
            restart_backoff: Duration::from_millis(250),
            max_restart_backoff: Duration::from_secs(10),
            shutdown_timeout: Duration::from_secs(2),
            write_timeout: Duration::from_secs(5),
            max_restarts: 3,
            max_consecutive_bad_lines: 3,
        }
    }
}

impl SessionConfig {
    fn validate(&self) -> Result<(), SessionError> {
        let durations = [
            self.startup_timeout,
            self.response_timeout,
            self.restart_backoff,
            self.max_restart_backoff,
            self.shutdown_timeout,
            self.write_timeout,
        ];
        if self.max_restarts > 32
            || durations
                .iter()
                .any(|d| d.is_zero() || *d > Duration::from_secs(86400))
        {
            return Err(SessionError::InvalidRequest(
                "timeouts must be in (0, 24h] and max_restarts <= 32".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum QueryEvent {
    Partial(AnalysisResponse),
    Final(AnalysisResponse),
    Restarted,
    Failed(QueryError),
    Terminated,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum QueryError {
    #[error("engine query failed: {0}")]
    Engine(String),
    #[error("engine returned no results for turn {0}")]
    NoResults(u32),
    #[error("engine session failed: {0}")]
    Session(String),
    #[error("engine session shut down")]
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    #[error("engine session is closed")]
    Closed,
    #[error("engine unavailable: {0}")]
    Unavailable(String),
    #[error("invalid analysis request: {0}")]
    InvalidRequest(String),
    #[error("engine session capacity reached")]
    Capacity,
    #[error("process cleanup failed: {0}")]
    Cleanup(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineStatus {
    Starting,
    Ready {
        version: String,
    },
    Restarting {
        attempt: u32,
        reason: String,
    },
    Failed {
        reason: String,
        stderr_tail: Vec<String>,
    },
    Stopped,
}

/// Cheap to clone: only command and status channels, never engine state.
#[derive(Clone)]
pub struct EngineHandle {
    commands: mpsc::Sender<actor::Command>,
    status: watch::Receiver<EngineStatus>,
}

/// Must be called within a Tokio runtime.
pub fn spawn(transport: TransportSpec, cfg: SessionConfig) -> EngineHandle {
    let (commands, receiver) = mpsc::channel(64);
    let (status, status_rx) = watch::channel(EngineStatus::Starting);
    tokio::spawn(actor::run(transport, cfg, receiver, status));
    EngineHandle {
        commands,
        status: status_rx,
    }
}

impl EngineHandle {
    pub async fn submit(&self, req: AnalysisRequest) -> Result<QuerySubscription, SessionError> {
        req.validate()?;
        let (reply, received) = oneshot::channel();
        self.commands
            .send(actor::Command::Submit(Box::new(req), reply))
            .await
            .map_err(|_| SessionError::Closed)?;
        received.await.map_err(|_| SessionError::Closed)?
    }

    /// Uses the subscription's stable initial id, including after engine restarts.
    /// Unknown/already finished ids are an idempotent success.
    pub async fn terminate(&self, id: QueryId) -> Result<(), SessionError> {
        let (reply, received) = oneshot::channel();
        self.commands
            .send(actor::Command::Terminate(id, reply))
            .await
            .map_err(|_| SessionError::Closed)?;
        received.await.map_err(|_| SessionError::Closed)?
    }

    pub fn status(&self) -> watch::Receiver<EngineStatus> {
        self.status.clone()
    }

    /// Stops this entire session (including clones) and waits for process reaping.
    pub async fn shutdown(self) -> Result<(), SessionError> {
        if *self.status.borrow() == EngineStatus::Stopped {
            return Ok(());
        }
        let (reply, received) = oneshot::channel();
        self.commands
            .send(actor::Command::Shutdown(reply))
            .await
            .map_err(|_| SessionError::Closed)?;
        received.await.map_err(|_| SessionError::Closed)?
    }
}
