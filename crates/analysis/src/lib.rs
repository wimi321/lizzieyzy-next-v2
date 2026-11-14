//! Actor-owned live analysis. The engine must report Black-perspective values;
//! this layer preserves those values without converting them again.

mod actor;
mod cache;
mod delivery;
mod limits;
pub mod persistent_cache;
mod request;
pub mod review;
mod speed;
mod throttle;

pub use delivery::AnalysisSubscription;
pub use limits::{AnalysisLimits, LimitReason};
pub use request::play_request;

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

#[derive(Clone, Debug, PartialEq)]
pub struct LiveSettings {
    /// Overrides are part of the caller-provided immutable engine fingerprint.
    pub query_rules: Option<katago_protocol::Rules>,
    pub query_komi: Option<f64>,
    pub max_visits: u64,
    pub report_interval: Duration,
    pub include_ownership: bool,
    pub include_policy: bool,
}

impl Default for LiveSettings {
    fn default() -> Self {
        Self {
            query_rules: None,
            query_komi: None,
            max_visits: u64::from(u32::MAX),
            report_interval: Duration::from_millis(150),
            include_ownership: true,
            include_policy: false,
        }
    }
}

impl LiveSettings {
    fn validate(&self) -> Result<(), AnalysisError> {
        if self
            .query_komi
            .is_some_and(|komi| !komi.is_finite() || komi.abs() > 400.0)
        {
            return Err(AnalysisError::InvalidSettings(
                "komi override must be within -400..400",
            ));
        }
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnalysisExtras {
    pub policy: bool,
    pub moves_ownership: bool,
}

#[derive(Clone, Debug)]
pub struct AnalysisConfig {
    /// Caller-defined identity of engine version, model and evaluation settings.
    /// Immutable for this coordinator; use a new coordinator when it changes.
    pub engine_fingerprint: String,
    pub cache_capacity: usize,
    pub cache_enabled: bool,
    pub live: LiveSettings,
    pub pondering: bool,
    pub limits: AnalysisLimits,
    pub extras: AnalysisExtras,
    pub pure_network: bool,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            engine_fingerprint: String::new(),
            cache_capacity: 128,
            cache_enabled: true,
            live: LiveSettings::default(),
            pondering: true,
            limits: AnalysisLimits::default(),
            extras: AnalysisExtras::default(),
            pure_network: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotSource {
    Cache,
    Live,
    /// A one-visit supplement, merged with the current normal display.
    Extras,
    /// Transient raw network output; never a searched/cache result.
    PureNetwork,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisSnapshot {
    pub revision: Revision,
    pub node_id: NodeId,
    pub key: AnalysisKey,
    pub response: AnalysisResponse,
    pub source: SnapshotSource,
    /// Current live query's visits per second; cached results have no speed.
    pub visits_per_second: Option<f64>,
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
    #[error("whole-game analysis failed: {0}")]
    Review(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnalysisStatus {
    Idle,
    Analyzing,
    Restarting,
    Limited(LimitReason),
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
    config.limits.validate()?;
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
        self.focus_with_cached(focus, None).await
    }

    /// Atomically seed a document-retained response before focusing. The caller
    /// supplies a response for this exact position and engine fingerprint.
    /// Seeding never replaces a deeper LRU entry or a published same-focus
    /// snapshot; returning after eviction still displays the retained result
    /// before live analysis, with the usual no-regression display floor.
    pub async fn focus_with_cached(
        &self,
        focus: Focus,
        cached: Option<AnalysisResponse>,
    ) -> Result<(), AnalysisError> {
        self.focus_with_cached_and_resume(focus, cached, false)
            .await
    }

    /// Apply a focus transition and optionally resume an existing limit pause
    /// atomically, before the new query can produce results or hit its limit.
    /// Same-position resumes retain the Java playout exemption; navigating to
    /// another position always starts with that position's normal thresholds.
    pub async fn focus_with_cached_and_resume(
        &self,
        focus: Focus,
        cached: Option<AnalysisResponse>,
        resume_limited: bool,
    ) -> Result<(), AnalysisError> {
        if focus.key != focus.position.analysis_key() {
            return Err(AnalysisError::KeyMismatch);
        }
        self.send(actor::Input::Focus(
            Box::new(focus),
            cached.map(Box::new),
            resume_limited,
        ))
        .await
    }

    /// Invalidate an unavailable/illegal display position without changing any
    /// live intent, limit pause or runtime query mode.
    pub async fn clear_focus(&self) -> Result<(), AnalysisError> {
        self.send(actor::Input::ClearFocus).await
    }

    pub async fn set_pondering(&self, enabled: bool) -> Result<(), AnalysisError> {
        self.send(actor::Input::SetPondering(enabled)).await
    }

    /// Disable both reading and writing the coordinator's in-memory cache.
    /// Existing results stay retained and become available again on re-enable.
    pub async fn set_cache_enabled(&self, enabled: bool) -> Result<(), AnalysisError> {
        self.send(actor::Input::SetCacheEnabled(enabled)).await
    }

    /// Forget old document analysis without resetting user intent or a limit
    /// pause. Any ongoing query restarts so old output cannot refill the view.
    pub async fn discard_cached_analysis(&self) -> Result<(), AnalysisError> {
        self.send(actor::Input::DiscardCachedAnalysis).await
    }

    /// Resume a limit-paused query without changing the persisted pondering
    /// intent. Calling this while already running is a no-op.
    pub async fn resume(&self) -> Result<(), AnalysisError> {
        self.send(actor::Input::Resume).await
    }

    /// Apply thresholds without restarting the current search or its clock.
    /// Updating limits does not implicitly resume a limit-paused query.
    pub async fn set_limits(&self, limits: AnalysisLimits) -> Result<(), AnalysisError> {
        limits.validate()?;
        self.send(actor::Input::SetLimits(limits)).await
    }

    /// Invalid settings leave the currently running query untouched.
    /// Changed settings reset the current focus's published visits floor.
    pub async fn set_live_settings(&self, settings: LiveSettings) -> Result<(), AnalysisError> {
        settings.validate()?;
        self.send(actor::Input::SetLiveSettings(settings)).await
    }

    /// Runtime-only query shape. A missing payload is fetched with one visit
    /// without changing the ordinary live search intent or a limit pause.
    pub async fn set_analysis_extras(&self, extras: AnalysisExtras) -> Result<(), AnalysisError> {
        self.send(actor::Input::SetAnalysisExtras(extras)).await
    }

    /// Temporarily replace the foreground query with raw one-visit output.
    /// The caller arbitrates whole-game/batch work before enabling this mode.
    pub async fn set_pure_network(&self, enabled: bool) -> Result<(), AnalysisError> {
        self.send(actor::Input::SetPureNetwork(enabled)).await
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
