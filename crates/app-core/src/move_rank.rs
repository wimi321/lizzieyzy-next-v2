//! Move quality thresholds shared by problem lists and loss bars.
//!
//! Matches Java's `MoveRankDefinition` and `MoveRankEvaluationMode`. Losses
//! are positive quantities; winrate thresholds are percentage points.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub enum MoveRank {
    Best,
    Good,
    Normal,
    Inaccuracy,
    Mistake,
    Blunder,
}

impl MoveRank {
    pub fn severity_level(self) -> u8 {
        match self {
            Self::Best => 0,
            Self::Good => 1,
            Self::Normal => 2,
            Self::Inaccuracy => 3,
            Self::Mistake => 4,
            Self::Blunder => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum MoveRankEvaluationMode {
    /// Prefer score, with winrate guarding against Mistake or Blunder losses.
    #[default]
    Auto,
    Score,
    Winrate,
    /// Use whichever metric reaches the more severe rank.
    Combined,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct MoveRankSettings {
    pub mode: MoveRankEvaluationMode,
    pub winrate_thresholds: [f64; 5],
    pub score_thresholds: [f64; 5],
}

impl Default for MoveRankSettings {
    fn default() -> Self {
        Self {
            mode: MoveRankEvaluationMode::Auto,
            winrate_thresholds: [1.0, 3.0, 6.0, 12.0, 24.0],
            score_thresholds: [0.5, 1.5, 3.0, 6.0, 12.0],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MoveRankError {
    #[error("winrate thresholds must be finite, positive, strictly increasing and at most 100")]
    InvalidWinrateThresholds,
    #[error("score thresholds must be finite, positive and strictly increasing")]
    InvalidScoreThresholds,
}

impl MoveRankSettings {
    pub fn validate(&self) -> Result<(), MoveRankError> {
        if !valid_thresholds(&self.winrate_thresholds)
            || self.winrate_thresholds.iter().any(|value| *value > 100.0)
        {
            return Err(MoveRankError::InvalidWinrateThresholds);
        }
        if !valid_thresholds(&self.score_thresholds) {
            return Err(MoveRankError::InvalidScoreThresholds);
        }
        Ok(())
    }

    /// Classify losses using inclusive threshold boundaries. Improvements do
    /// not count as losses. Missing score data follows Java's mode-specific
    /// behavior: Auto falls back to winrate and Score yields Best.
    pub fn classify_losses(&self, winrate_loss: f64, score_loss: Option<f64>) -> MoveRank {
        let winrate_loss = winrate_loss.max(0.0);
        let score_loss = score_loss.map(|loss| loss.max(0.0));
        let ranks = [
            MoveRank::Good,
            MoveRank::Normal,
            MoveRank::Inaccuracy,
            MoveRank::Mistake,
            MoveRank::Blunder,
        ];
        for (index, rank) in ranks.into_iter().enumerate().rev() {
            let reaches_winrate = winrate_loss >= self.winrate_thresholds[index];
            let reaches_score = score_loss.is_some_and(|loss| loss >= self.score_thresholds[index]);
            let reaches = match self.mode {
                MoveRankEvaluationMode::Score => reaches_score,
                MoveRankEvaluationMode::Winrate => reaches_winrate,
                MoveRankEvaluationMode::Combined => reaches_winrate || reaches_score,
                MoveRankEvaluationMode::Auto => {
                    if score_loss.is_none() {
                        reaches_winrate
                    } else {
                        reaches_score || (rank >= MoveRank::Mistake && reaches_winrate)
                    }
                }
            };
            if reaches {
                return rank;
            }
        }
        MoveRank::Best
    }
}

fn valid_thresholds(values: &[f64; 5]) -> bool {
    values.iter().all(|value| value.is_finite() && *value > 0.0)
        && values.windows(2).all(|pair| pair[0] < pair[1])
}

#[cfg(test)]
mod tests;
