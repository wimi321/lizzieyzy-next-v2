//! Persisted single-position limits and the effective, temporary analysis state.

use crate::{AppCore, AppError, actor};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct AnalysisLimitSettings {
    pub limit_time: bool,
    pub max_analyze_time_seconds: u32,
    pub limit_playout: bool,
    #[ts(type = "number")]
    pub limit_playouts: u64,
}

impl Default for AnalysisLimitSettings {
    fn default() -> Self {
        Self {
            limit_time: true,
            max_analyze_time_seconds: 600,
            limit_playout: false,
            limit_playouts: 100_000,
        }
    }
}

impl AnalysisLimitSettings {
    pub fn validate(&self) -> Result<(), analysis::AnalysisError> {
        if self.max_analyze_time_seconds == 0 {
            return Err(analysis::AnalysisError::InvalidSettings(
                "max_analyze_time_seconds must be positive; use limit_time to disable",
            ));
        }
        if self.limit_playouts == 0 || self.limit_playouts > 9_007_199_254_740_991 {
            return Err(analysis::AnalysisError::InvalidSettings(
                "limit_playouts must be a positive JavaScript-safe integer",
            ));
        }
        Ok(())
    }
}

impl From<AnalysisLimitSettings> for analysis::AnalysisLimits {
    fn from(value: AnalysisLimitSettings) -> Self {
        Self {
            limit_time: value.limit_time,
            max_analyze_time_seconds: value.max_analyze_time_seconds,
            limit_playout: value.limit_playout,
            limit_playouts: value.limit_playouts,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AnalysisPauseReason {
    User,
    TimeLimit,
    PlayoutLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AnalysisState {
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub pure_network: bool,
    /// Effective permission to ponder, not a guarantee that an engine is ready.
    pub pondering: bool,
    pub pause_reason: Option<AnalysisPauseReason>,
}

pub(crate) enum Request {
    Status(oneshot::Sender<AnalysisState>),
    Set(
        AnalysisLimitSettings,
        oneshot::Sender<Result<AnalysisLimitSettings, AppError>>,
    ),
    Toggle(oneshot::Sender<Result<AnalysisState, AppError>>),
}

impl AppCore {
    pub async fn analysis_limits(&self) -> Result<AnalysisLimitSettings, AppError> {
        Ok(self.settings().await?.analysis_limits)
    }

    pub async fn set_analysis_limits(
        &self,
        settings: AnalysisLimitSettings,
    ) -> Result<AnalysisLimitSettings, AppError> {
        settings.validate()?;
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::AnalysisLimit(Request::Set(settings, reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    pub async fn analysis_state(&self) -> Result<AnalysisState, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::AnalysisLimit(Request::Status(reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }

    /// Space-bar semantics: an automatic pause resumes without persisting false.
    pub async fn toggle_pondering(&self) -> Result<AnalysisState, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::AnalysisLimit(Request::Toggle(reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}
