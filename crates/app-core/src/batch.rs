//! Multi-file review commands. The application actor owns the entire queue.
use crate::{AppCore, AppError, WholeGameOptions, WholeGameProgress, WholeGameScope, actor};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::oneshot;
use ts_rs::TS;

/// Local wall-clock formatting is injectable; analysis scheduling uses Tokio time.
pub trait BatchClock: std::fmt::Debug + Send + Sync {
    fn local_timestamp(&self) -> String;
}

#[derive(Debug)]
pub struct LocalBatchClock;
impl BatchClock for LocalBatchClock {
    fn local_timestamp(&self) -> String {
        chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BatchOutput {
    pub suffix: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub directory: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BatchOptions {
    pub files: Vec<PathBuf>,
    #[ts(type = "number")]
    pub visits: u64,
    #[serde(default)]
    #[ts(as = "Option<WholeGameScope>", optional)]
    pub scope: WholeGameScope,
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub force: bool,
    pub output: BatchOutput,
}

impl BatchOptions {
    pub(crate) fn validate(&self) -> Result<(), AppError> {
        WholeGameOptions {
            visits: self.visits,
            scope: self.scope,
            force: self.force,
        }
        .validate()?;
        let suffix = &self.output.suffix;
        if suffix.trim().is_empty()
            || suffix.chars().count() > 64
            || suffix
                .chars()
                .any(|c| c.is_control() || "/\\<>:\"|?*".contains(c))
        {
            return Err(BatchError::InvalidSuffix.into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum BatchPhase {
    #[default]
    Idle,
    Running,
    Paused,
    Stopped,
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum BatchItemState {
    Waiting,
    Running {
        #[ts(optional = nullable)]
        progress: Option<WholeGameProgress>,
    },
    Completed {
        #[ts(as = "Option<PathBuf>", optional)]
        output: PathBuf,
    },
    Failed {
        #[ts(as = "Option<String>", optional)]
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BatchItem {
    #[ts(as = "Option<u32>", optional)]
    pub id: u32,
    #[ts(as = "Option<PathBuf>", optional)]
    pub path: PathBuf,
    #[ts(as = "Option<BatchItemState>", optional)]
    pub state: BatchItemState,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BatchAnalysisStatus {
    #[ts(as = "Option<BatchPhase>", optional)]
    pub phase: BatchPhase,
    #[ts(as = "Option<Vec<BatchItem>>", optional)]
    pub items: Vec<BatchItem>,
    /// Zero-based index in the current snapshot, independent of the stable item ID.
    #[ts(optional = nullable)]
    pub current_index: Option<usize>,
    #[ts(optional = nullable)]
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum BatchMove {
    Up,
    Down,
    First,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "type", content = "data")]
pub enum BatchError {
    #[error(
        "batch output suffix must contain 1-64 characters without control or filename separators"
    )]
    InvalidSuffix,
    #[error("batch queue has no waiting items")]
    EmptyQueue,
    #[error("batch is not paused")]
    NotPaused,
    #[error("batch item does not exist or is not waiting")]
    NotWaiting,
    #[error("batch item identifiers are exhausted")]
    IdLimit,
    #[error("batch clock must return a 14-digit local timestamp")]
    InvalidTimestamp,
}

pub(crate) enum Request {
    Start(BatchOptions),
    Pause,
    Resume,
    Stop,
    Add(Vec<PathBuf>),
    Remove(u32),
    Move(u32, BatchMove),
    Clear,
}

impl AppCore {
    /// Adds files to the retained queue and starts its waiting items. Pass no
    /// files to restart waiting items left by stop; completed items stay complete.
    pub async fn start_batch_analysis(
        &self,
        options: BatchOptions,
    ) -> Result<BatchAnalysisStatus, AppError> {
        self.batch_request(Request::Start(options)).await
    }
    pub async fn pause_batch_analysis(&self) -> Result<BatchAnalysisStatus, AppError> {
        self.batch_request(Request::Pause).await
    }
    pub async fn resume_batch_analysis(&self) -> Result<BatchAnalysisStatus, AppError> {
        self.batch_request(Request::Resume).await
    }
    pub async fn stop_batch_analysis(&self) -> Result<BatchAnalysisStatus, AppError> {
        self.batch_request(Request::Stop).await
    }
    pub async fn add_batch_analysis_files(
        &self,
        files: Vec<PathBuf>,
    ) -> Result<BatchAnalysisStatus, AppError> {
        self.batch_request(Request::Add(files)).await
    }
    pub async fn remove_batch_analysis_item(
        &self,
        id: u32,
    ) -> Result<BatchAnalysisStatus, AppError> {
        self.batch_request(Request::Remove(id)).await
    }
    pub async fn move_batch_analysis_item(
        &self,
        id: u32,
        direction: BatchMove,
    ) -> Result<BatchAnalysisStatus, AppError> {
        self.batch_request(Request::Move(id, direction)).await
    }
    pub async fn clear_batch_analysis(&self) -> Result<BatchAnalysisStatus, AppError> {
        self.batch_request(Request::Clear).await
    }
    pub async fn batch_analysis_status(&self) -> Result<BatchAnalysisStatus, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::BatchStatus(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }
    async fn batch_request(&self, request: Request) -> Result<BatchAnalysisStatus, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::Batch(request, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}
