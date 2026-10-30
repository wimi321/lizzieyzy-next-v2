//! Actor-owned live analysis. The engine must report Black-perspective values;
//! this layer preserves those values without converting them again.

mod actor;
mod cache;
mod delivery;
mod request;
mod throttle;

pub use delivery::AnalysisSubscription;

use document::{AnalysisKey, GamePosition, NodeId, Revision};
use engine_session::{EngineHandle, QueryError, SessionError};
use katago_protocol::AnalysisResponse;
use std::{num::NonZeroUsize, time::Duration};
use tokio::sync::{mpsc, oneshot, watch};

#[derive(Clone, Debug, PartialEq)]
pub struct Focus {
    pub revision: Revision,
    pub node_id: NodeId,
    pub position: GamePosition,
    pub key: AnalysisKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveSettings {
    pub max_visits: u64,
    pub report_interval: Duration,
    pub include_ownership: bool,
    pub include_policy: bool,
}

impl Default for LiveSettings {
    fn default() -> Self {
        Self {
            max_visits: u64::from(u32::MAX),
            report_interval: Duration::from_millis(150),
            include_ownership: true,
            include_policy: false,
        }
    }
}

impl LiveSettings {
    fn validate(&self) -> Result<(), AnalysisError> {
        if self.max_visits == 0 {
            return Err(AnalysisError::InvalidSettings(
                "max_visits must be positive",
            ));
        }
        if self.report_interval.is_zero() || self.report_interval > Duration::from_secs(86400) {
            return Err(AnalysisError::InvalidSettings(
                "report_interval must be in (0, 24h]",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct AnalysisConfig {
    /// Caller-defined identity of engine version, model and evaluation settings.
    /// Immutable for this coordinator; use a new coordinator when it changes.
    pub engine_fingerprint: String,
    pub cache_capacity: usize,
    pub live: LiveSettings,
    pub pondering: bool,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            engine_fingerprint: String::new(),
            cache_capacity: 128,
            live: LiveSettings::default(),
            pondering: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotSource {
    Cache,
    Live,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisSnapshot {
    pub revision: Revision,
    pub node_id: NodeId,
    pub key: AnalysisKey,
    pub response: AnalysisResponse,
    pub source: SnapshotSource,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AnalysisError {
    #[error("analysis coordinator is closed")]
    Closed,
    #[error("invalid live analysis settings: {0}")]
    InvalidSettings(&'static str),
    #[error("invalid analysis position: {0}")]
    InvalidPosition(&'static str),
    #[error("focus key does not match its position")]
    KeyMismatch,
    #[error(transparent)]
    Coordinate(#[from] go_core::CoordinateError),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error(transparent)]
    Query(#[from] QueryError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnalysisStatus {
    Idle,
    Analyzing,
    Restarting,
    Failed(AnalysisError),
    Stopped,
}

/// Cloneable command channels. Dropping the last handle cancels its query;
/// retaining a snapshot or status subscription does not keep the actor alive.
#[derive(Clone)]
pub struct AnalysisHandle {
    commands: mpsc::Sender<actor::Command>,
    status: watch::Receiver<AnalysisStatus>,
}

/// Start within a Tokio runtime. This coordinator borrows the engine session:
/// shutting it down cancels only its live query, never other session clients.
pub fn spawn(
    engine: EngineHandle,
    config: AnalysisConfig,
) -> Result<(AnalysisHandle, AnalysisSubscription), AnalysisError> {
    config.live.validate()?;
    let capacity = NonZeroUsize::new(config.cache_capacity).ok_or(
        AnalysisError::InvalidSettings("cache_capacity must be positive"),
    )?;
    let (commands, receiver) = mpsc::channel(64);
    let (status, status_rx) = watch::channel(AnalysisStatus::Idle);
    let (output, subscription) = delivery::channel();
    tokio::spawn(actor::run(
        engine, config, capacity, receiver, status, output,
    ));
    Ok((
        AnalysisHandle {
            commands,
            status: status_rx,
        },
        subscription,
    ))
}

impl AnalysisHandle {
    /// On success the focus transition has been applied, and unread snapshots
    /// from previous focuses can no longer be received.
    /// A revision-only change retains the query and immediately republishes
    /// its latest snapshot with the new revision, preserving display depth.
    pub async fn focus(&self, focus: Focus) -> Result<(), AnalysisError> {
        if focus.key != focus.position.analysis_key() {
            return Err(AnalysisError::KeyMismatch);
        }
        self.send(actor::Input::Focus(Box::new(focus))).await
    }

    pub async fn set_pondering(&self, enabled: bool) -> Result<(), AnalysisError> {
        self.send(actor::Input::SetPondering(enabled)).await
    }

    /// Invalid settings leave the currently running query untouched.
    /// Changed settings reset the current focus's published visits floor.
    pub async fn set_live_settings(&self, settings: LiveSettings) -> Result<(), AnalysisError> {
        settings.validate()?;
        self.send(actor::Input::SetLiveSettings(settings)).await
    }

    pub fn status(&self) -> watch::Receiver<AnalysisStatus> {
        self.status.clone()
    }

    pub async fn shutdown(self) -> Result<(), AnalysisError> {
        if *self.status.borrow() == AnalysisStatus::Stopped {
            return Ok(());
        }
        self.send(actor::Input::Shutdown).await
    }

    async fn send(&self, input: actor::Input) -> Result<(), AnalysisError> {
        let (reply, received) = oneshot::channel();
        self.commands
            .send(actor::Command { input, reply })
            .await
            .map_err(|_| AnalysisError::Closed)?;
        received.await.map_err(|_| AnalysisError::Closed)?
    }
}
