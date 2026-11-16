//! Explicit GPU activation; the document actor owns the whole transaction.
use crate::{
    AccelerationBackend, AppCore, AppError, DiagnosticRepair, EngineDiagnostics, RepairResources,
    Settings, actor::Command,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

pub(crate) mod deadline;
pub(crate) mod monitor;
pub(crate) mod output;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AccelerationPhase {
    Downloading,
    Starting,
    Tuning,
    Ready,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AccelerationProgress {
    #[ts(as = "Option<AccelerationBackend>", optional)]
    pub backend: AccelerationBackend,
    #[ts(as = "Option<AccelerationPhase>", optional)]
    pub phase: AccelerationPhase,
    #[ts(as = "Option<Vec<String>>", optional)]
    pub output_tail: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum AccelerationError {
    #[error("acceleration, benchmark or engine trial is already running")]
    Busy,
    #[error("GPU acceleration is available only on Windows x64")]
    UnsupportedPlatform,
    #[error("this acceleration backend is not supported")]
    UnsupportedBackend,
    #[error("no local main engine is selected")]
    NoEngine,
    #[error("acceleration was cancelled; output: {stderr_tail:?}")]
    Cancelled { stderr_tail: Vec<String> },
    #[error("acceleration startup timed out; output: {stderr_tail:?}")]
    TimedOut { stderr_tail: Vec<String> },
    #[error("acceleration failed: {reason}; output: {stderr_tail:?}")]
    Failed {
        reason: String,
        stderr_tail: Vec<String>,
    },
}

impl AppCore {
    /// Downloads a pinned runtime, tries it, and commits only after Ready.
    pub async fn enable_acceleration(
        &self,
        backend: AccelerationBackend,
    ) -> Result<Settings, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::EnableAcceleration(backend, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    /// The host supplies the same fixed bundled paths as diagnostic repair.
    pub async fn disable_acceleration(
        &self,
        resources: RepairResources,
    ) -> Result<EngineDiagnostics, AppError> {
        self.repair_engine(DiagnosticRepair::UseBundledProgram, resources)
            .await
    }

    pub async fn dismiss_acceleration_hint(&self) -> Result<(), AppError> {
        self.send(crate::actor::Input::DismissAccelerationHint)
            .await
            .map(|_| ())
    }

    /// Most recent activation's bounded diagnostics, also available after failure.
    pub async fn acceleration_output(&self) -> Result<Vec<String>, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::AccelerationOutput(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }
}

#[cfg(test)]
mod tests;
