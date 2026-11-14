//! HumanSL coaching values. Session ownership and scheduling stay in the game actor.
mod rating;
mod selection;
mod timing;

pub(crate) use rating::{best_hint, rate_move, reviewed_lead};
pub(crate) use selection::{
    PolicyCandidate, candidate_pool, human_pass_share, human_top_share, needs_deeper,
    pass_check_needed, pass_override, sample_raw_policy, select_searched_move,
};
pub(crate) use timing::{adaptive_visits, should_resign, think_delay};

use crate::{ColorView, GameEndReason, MoveView};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum CoachRank {
    Kyu(u8),
    Dan(u8),
}

impl Default for CoachRank {
    fn default() -> Self {
        Self::Dan(3)
    }
}

impl CoachRank {
    pub fn validate(self) -> Result<(), CoachError> {
        if matches!(self, Self::Kyu(1..=20) | Self::Dan(1..=9)) {
            Ok(())
        } else {
            Err(CoachError::InvalidSetup(
                "rank must be 20k..1k or 1d..9d".into(),
            ))
        }
    }

    pub fn profile(self) -> String {
        match self {
            Self::Kyu(rank) => format!("rank_{rank}k"),
            Self::Dan(rank) => format!("rank_{rank}d"),
        }
    }

    pub fn raw_policy(self) -> bool {
        !matches!(self, Self::Dan(6..=9))
    }

    pub fn elite(self) -> bool {
        matches!(self, Self::Dan(7..=9))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CoachMode {
    #[default]
    LiveFeedback,
    ReviewAfter,
    LiveAnalysis,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CoachColor {
    #[default]
    Random,
    Black,
    White,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CoachPace {
    #[default]
    Natural,
    Fast,
    Slow,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct CoachSetup {
    #[ts(as = "Option<CoachRank>", optional)]
    pub rank: CoachRank,
    #[ts(as = "Option<CoachMode>", optional)]
    pub mode: CoachMode,
    #[ts(as = "Option<CoachColor>", optional)]
    pub color: CoachColor,
    #[ts(as = "Option<CoachPace>", optional)]
    pub pace: CoachPace,
    #[ts(as = "Option<u8>", optional)]
    pub handicap: u8,
    #[ts(as = "Option<bool>", optional)]
    pub from_current_position: bool,
    #[ts(as = "Option<u8>", optional)]
    pub width: u8,
    #[ts(as = "Option<u8>", optional)]
    pub height: u8,
    #[ts(as = "Option<bool>", optional)]
    pub auto_save: bool,
}

impl Default for CoachSetup {
    fn default() -> Self {
        Self {
            rank: CoachRank::default(),
            mode: CoachMode::default(),
            color: CoachColor::default(),
            pace: CoachPace::default(),
            handicap: 0,
            from_current_position: false,
            width: 19,
            height: 19,
            auto_save: true,
        }
    }
}

impl CoachSetup {
    pub fn validate(&self) -> Result<(), CoachError> {
        self.rank.validate()?;
        if self.handicap > 9 {
            return Err(CoachError::InvalidSetup("handicap must be 0..9".into()));
        }
        go_core::BoardSize::new(self.width, self.height)
            .map_err(|error| CoachError::InvalidSetup(error.to_string()))?;
        if !self.from_current_position
            && self.handicap >= 2
            && (self.width != self.height || ![9, 13, 19].contains(&self.width))
        {
            return Err(CoachError::InvalidSetup(
                "fixed handicap requires a 9, 13 or 19 square board".into(),
            ));
        }
        Ok(())
    }

    pub fn persisted(&self) -> Self {
        Self {
            from_current_position: false,
            ..self.clone()
        }
    }

    pub fn komi(&self) -> f64 {
        if self.handicap > 0 { 0.5 } else { 7.5 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct HumanSlPlay {
    #[ts(as = "Option<CoachRank>", optional)]
    pub rank: CoachRank,
    #[ts(as = "Option<CoachPace>", optional)]
    pub pace: CoachPace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub enum CoachVerdict {
    Brilliant,
    Good,
    Slow,
    Problem,
    Blunder,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CoachBestMove {
    #[ts(as = "Option<MoveView>", optional)]
    pub mv: MoveView,
    #[ts(optional = nullable)]
    pub human_share: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum CoachHint {
    Reading,
    /// Zero-based row and column, from the top-left, in a 3 by 3 grid.
    Region {
        #[ts(as = "Option<u8>", optional)]
        row: u8,
        #[ts(as = "Option<u8>", optional)]
        column: u8,
    },
    Exact(CoachBestMove),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CoachDecision {
    Retry,
    ShowBest,
    Continue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CoachMoveRated {
    #[ts(as = "Option<u32>", optional)]
    pub node_id: u32,
    #[ts(as = "Option<u32>", optional)]
    pub move_number: u32,
    #[ts(as = "Option<ColorView>", optional)]
    pub color: ColorView,
    #[ts(as = "Option<CoachVerdict>", optional)]
    pub verdict: CoachVerdict,
    /// Rank-aware probabilities and change are always from Black's perspective.
    #[ts(as = "Option<f64>", optional)]
    pub winrate_before: f64,
    #[ts(as = "Option<f64>", optional)]
    pub winrate_after: f64,
    #[ts(as = "Option<f64>", optional)]
    pub winrate_change: f64,
    /// Nonnegative loss attributable to the player of this move.
    #[ts(as = "Option<f64>", optional)]
    pub points_lost: f64,
    #[ts(optional = nullable)]
    pub human_share: Option<f64>,
    #[ts(as = "Option<CoachBestMove>", optional)]
    pub best: CoachBestMove,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CoachWorstMove {
    #[ts(as = "Option<u32>", optional)]
    pub node_id: u32,
    #[ts(as = "Option<u32>", optional)]
    pub move_number: u32,
    #[ts(as = "Option<CoachVerdict>", optional)]
    pub verdict: CoachVerdict,
    #[ts(as = "Option<f64>", optional)]
    pub points_lost: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CoachSummary {
    #[ts(optional = nullable)]
    pub result: Option<String>,
    #[ts(optional = nullable)]
    pub end_reason: Option<GameEndReason>,
    #[ts(as = "Option<u32>", optional)]
    pub total_moves: u32,
    #[ts(as = "Option<u32>", optional)]
    pub blunders: u32,
    #[ts(as = "Option<u32>", optional)]
    pub hints: u32,
    #[ts(as = "Option<u32>", optional)]
    pub take_backs: u32,
    #[ts(as = "Option<Vec<CoachWorstMove>>", optional)]
    pub worst_moves: Vec<CoachWorstMove>,
}

impl CoachSummary {
    pub fn from_ratings(
        result: Option<String>,
        end_reason: Option<GameEndReason>,
        total_moves: u32,
        hints: u32,
        take_backs: u32,
        ratings: &[CoachMoveRated],
    ) -> Self {
        let mut worst_moves: Vec<_> = ratings
            .iter()
            .map(|rating| CoachWorstMove {
                node_id: rating.node_id,
                move_number: rating.move_number,
                verdict: rating.verdict,
                points_lost: rating.points_lost,
            })
            .collect();
        worst_moves.sort_by(|a, b| {
            b.points_lost
                .total_cmp(&a.points_lost)
                .then_with(|| a.move_number.cmp(&b.move_number))
        });
        worst_moves.truncate(3);
        Self {
            result,
            end_reason,
            total_moves,
            hints,
            take_backs,
            blunders: ratings
                .iter()
                .filter(|r| r.verdict == CoachVerdict::Blunder)
                .count() as u32,
            worst_moves,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CoachView {
    #[ts(as = "Option<bool>", optional)]
    pub active: bool,
    #[ts(as = "Option<CoachSetup>", optional)]
    pub setup: CoachSetup,
    #[ts(as = "Option<ColorView>", optional)]
    pub human_color: ColorView,
    #[ts(as = "Option<bool>", optional)]
    pub awaiting_decision: bool,
    #[ts(as = "Option<u32>", optional)]
    pub hints: u32,
    #[ts(as = "Option<u32>", optional)]
    pub take_backs: u32,
    #[ts(optional = nullable)]
    pub last_rating: Option<CoachMoveRated>,
    #[ts(optional = nullable)]
    pub summary: Option<CoachSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "type", content = "data")]
pub enum CoachError {
    #[error("invalid coach setup: {0}")]
    InvalidSetup(String),
    #[error("the HumanSL model is missing")]
    ModelMissing,
    #[error("the HumanSL model failed integrity verification")]
    ModelCorrupt,
    #[error("a ready main analysis engine is required")]
    NoEngine,
    #[error("HumanSL requires KataGo 1.15.0 or later; found {0}")]
    UnsupportedVersion(String),
    #[error("TensorRT does not support this HumanSL coaching backend")]
    TensorRtUnsupported,
    #[error("failed to load the HumanSL model: {reason}")]
    HumanModelLoadFailed { reason: String },
    #[error("there is no active coaching session")]
    NotActive,
    #[error("hints are only available in live feedback mode")]
    HintUnavailable,
    #[error("hints are only available on the human player's turn")]
    NotHumanTurn,
    #[error("a coaching decision is required before continuing")]
    AwaitingDecision,
    #[error("there is no coaching decision pending")]
    NoDecisionPending,
    #[error("the engine did not provide a valid HumanSL policy")]
    MissingHumanPolicy,
    #[error("the engine did not provide a legal move")]
    NoLegalMove,
}

#[cfg(test)]
mod tests;
