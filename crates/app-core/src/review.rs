//! Whole-game review commands and document-owned results for downstream review.

use crate::{AnalysisView, AppCore, AppError, actor};
use document::{AnalysisKey, NodeId};
use katago_protocol::AnalysisResponse;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum WholeGamePreset {
    Quick,
    #[default]
    Standard,
    Deep,
    Professional,
}

impl WholeGamePreset {
    pub const fn visits(self) -> u64 {
        match self {
            Self::Quick => 500,
            Self::Standard => 1000,
            Self::Deep => 3000,
            Self::Professional => 10000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct WholeGameOptions {
    #[ts(type = "number")]
    pub visits: u64,
}

impl Default for WholeGameOptions {
    fn default() -> Self {
        Self::from(WholeGamePreset::default())
    }
}

impl From<WholeGamePreset> for WholeGameOptions {
    fn from(value: WholeGamePreset) -> Self {
        Self {
            visits: value.visits(),
        }
    }
}

impl WholeGameOptions {
    pub fn validate(self) -> Result<(), WholeGameError> {
        if !(500..=1_000_000).contains(&self.visits) {
            return Err(WholeGameError::InvalidVisits);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum WholeGameStatus {
    Running,
    Paused,
    Completed,
    Cancelled,
    Error { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WholeGameProgress {
    /// Includes positions already meeting the requested depth before starting.
    pub completed: usize,
    /// Number of nodes in the captured displayed line, including the root.
    pub total: usize,
    #[ts(type = "number")]
    pub visits: u64,
    pub status: WholeGameStatus,
}

#[derive(Debug, thiserror::Error)]
pub enum WholeGameError {
    #[error("whole-game visits must be between 500 and 1000000")]
    InvalidVisits,
    #[error("whole-game analysis requires an active engine")]
    NoEngine,
    #[error("no whole-game analysis has been started")]
    NotStarted,
    #[error("only a paused whole-game analysis can be resumed")]
    NotPaused,
    #[error("a position in the captured review line was removed or changed")]
    ChangedPosition,
}

/// Best complete protocol payload for a node in the current document and engine
/// fingerprint. This store is independent of the live coordinator's LRU.
/// `response.root_info` contains Black's winrate/score/visits; `move_infos`
/// retains every candidate, PV, and all other parsed protocol statistics.
/// P1-24/P1-25 should read this by NodeId, not reconstruct data from graph
/// summaries. The key identifies the exact replay context (including history).
/// Live partials may also improve this payload; consult `is_during_search` if
/// a consumer needs to distinguish a snapshot from a finished search.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredAnalysis {
    pub node_id: NodeId,
    pub key: AnalysisKey,
    pub response: AnalysisResponse,
}

pub(crate) enum ReviewInput {
    Start(WholeGameOptions),
    Pause,
    Resume,
    Cancel,
}

impl AppCore {
    /// Captures root -> current -> first-child leaf. Later navigation does not
    /// change the task's line. Returns immediately after submitting the work.
    pub async fn start_whole_game_analysis(
        &self,
        options: WholeGameOptions,
    ) -> Result<WholeGameProgress, AppError> {
        options.validate()?;
        self.review_command(ReviewInput::Start(options)).await
    }

    pub async fn pause_whole_game_analysis(&self) -> Result<WholeGameProgress, AppError> {
        self.review_command(ReviewInput::Pause).await
    }

    pub async fn resume_whole_game_analysis(&self) -> Result<WholeGameProgress, AppError> {
        self.review_command(ReviewInput::Resume).await
    }

    pub async fn cancel_whole_game_analysis(&self) -> Result<WholeGameProgress, AppError> {
        self.review_command(ReviewInput::Cancel).await
    }

    pub async fn whole_game_progress(&self) -> Result<Option<WholeGameProgress>, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::ReviewProgress(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }

    /// Current-document view of a retained result, even after LRU eviction.
    /// Revision is the application's revision at the time of this read.
    pub async fn whole_game_result(&self, node_id: u32) -> Result<Option<AnalysisView>, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::ReviewResult(node_id, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    /// Full protocol result for Rust consumers (problem moves/strength rating).
    /// Removed or changed nodes return None. Opening another document or
    /// changing the engine fingerprint invalidates the entire retained store.
    pub async fn whole_game_analysis(
        &self,
        node_id: u32,
    ) -> Result<Option<StoredAnalysis>, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::ReviewAnalysis(node_id, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }

    async fn review_command(&self, input: ReviewInput) -> Result<WholeGameProgress, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::Review(input, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

#[cfg(test)]
mod tests;
