//! Read-only move losses derived from the document-owned P1-23 results.

use crate::{AppCore, AppError, ColorView, MoveRank, MoveRankSettings, MoveView, actor};
use go_core::{BoardSize, Color, Move};
use katago_protocol::AnalysisResponse;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ProblemMoveSort {
    /// Java ProblemListSort: winrate loss descending, then move number ascending.
    #[default]
    LossDesc,
    MoveAsc,
    MoveDesc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct ProblemMovesQuery {
    pub minimum_rank: MoveRank,
    /// None includes both players. Filtering never removes loss-bar points.
    pub color: Option<ColorView>,
    pub sort: ProblemMoveSort,
}

impl Default for ProblemMovesQuery {
    fn default() -> Self {
        Self {
            minimum_rank: MoveRank::Inaccuracy,
            color: None,
            sort: ProblemMoveSort::LossDesc,
        }
    }
}

/// Losses are nonnegative and belong to the player who made the move. These
/// are differences, not Black-perspective position evaluations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct MoveLoss {
    /// Percentage points (0..100), not a probability in 0..1.
    pub winrate_loss: f64,
    pub score_loss: f64,
    pub rank: MoveRank,
    pub is_best: bool,
    /// One-based engine order; None means absent from the candidates.
    pub candidate_rank: Option<u32>,
    /// Root visits after the move, matching Java's problem-list computation count.
    #[ts(type = "number")]
    pub visits: u64,
    #[ts(type = "number")]
    pub before_visits: u64,
}

impl MoveLoss {
    pub(crate) fn between(
        mv: Move,
        size: BoardSize,
        before: &AnalysisResponse,
        after: &AnalysisResponse,
        settings: &MoveRankSettings,
    ) -> Option<Self> {
        let best = before
            .move_infos
            .iter()
            .min_by_key(|candidate| candidate.order)?;
        if before.root_info.visits == 0 || after.root_info.visits == 0 || best.visits == 0 {
            return None;
        }
        // Invalid or unavailable evaluations must never masquerade as zero loss.
        if ![best.winrate, after.root_info.winrate]
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            || !best.score_lead.is_finite()
            || !after.root_info.score_lead.is_finite()
        {
            return None;
        }
        let best_move = Move::from_gtp(mv.color(), &best.coordinate, size).ok()?;
        let candidate_rank = before
            .move_infos
            .iter()
            .filter(|candidate| {
                Move::from_gtp(mv.color(), &candidate.coordinate, size).ok() == Some(mv)
            })
            .map(|candidate| candidate.order)
            .min()
            .and_then(|order| order.checked_add(1));
        let is_best = mv == best_move;
        let sign = if mv.color() == Color::Black {
            1.0
        } else {
            -1.0
        };
        let (winrate_loss, score_loss) = if is_best {
            (0.0, 0.0)
        } else {
            (
                (sign * (best.winrate - after.root_info.winrate) * 100.0).max(0.0),
                (sign * (best.score_lead - after.root_info.score_lead)).max(0.0),
            )
        };
        if !score_loss.is_finite() {
            return None;
        }
        Some(Self {
            winrate_loss,
            score_loss,
            rank: settings.classify_losses(winrate_loss, Some(score_loss)),
            is_best,
            candidate_rank,
            visits: after.root_info.visits,
            before_visits: before.root_info.visits,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct ProblemMoveView {
    pub node_id: u32,
    pub move_number: usize,
    pub color: ColorView,
    pub coordinate: String,
    pub loss: MoveLoss,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct MistakeBarPoint {
    pub node_id: u32,
    pub move_number: usize,
    pub mv: Option<MoveView>,
    /// None for missing analysis, non-move nodes, or incomparable positions.
    pub loss: Option<MoveLoss>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct ProblemMovesView {
    #[ts(type = "number")]
    pub revision: u64,
    pub settings: MoveRankSettings,
    pub query: ProblemMovesQuery,
    pub entries: Vec<ProblemMoveView>,
    /// Exactly the GraphView line and indexing, including root/empty nodes.
    pub error_bar: Vec<MistakeBarPoint>,
    pub current_index: usize,
}

impl AppCore {
    /// Read the current displayed line without navigating or starting analysis.
    /// Re-read on DocumentChanged, AnalysisUpdated or WholeGameProgress.
    pub async fn problem_moves(
        &self,
        query: ProblemMovesQuery,
    ) -> Result<ProblemMovesView, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::ProblemMoves(query, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    /// Persist thresholds/mode before activating them. Reclassification needs
    /// no new search and is announced by a same-revision DocumentChanged event.
    pub async fn set_move_rank_settings(
        &self,
        settings: MoveRankSettings,
    ) -> Result<MoveRankSettings, AppError> {
        settings.validate()?;
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::SetMoveRankSettings(settings, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

#[cfg(test)]
mod tests;
