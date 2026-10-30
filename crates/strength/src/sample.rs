use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Candidate values are in the moving player's perspective, winrate in percentage points.
/// Callers sort candidates by engine order, as the Java review candidate list does.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateInput {
    pub coordinate: String,
    pub winrate: f64,
    pub score_lead: Option<f64>,
    pub prior: f64,
    pub order: u32,
}

/// Only valid analyzed move pairs belong here. Fallback losses compare the
/// before/after roots in the moving player's perspective, as Java does.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveInput {
    pub move_number: u32,
    pub is_black: bool,
    pub coordinate: String,
    pub winrate_loss: f64,
    pub score_loss: Option<f64>,
    /// P1-24 move-rank severity, Best=0 through Blunder=5.
    pub move_rank: u8,
    pub candidates: Vec<CandidateInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum MoveCategory {
    Excellent,
    Great,
    Good,
    Inaccuracy,
    Mistake,
    Blunder,
}

impl MoveCategory {
    pub fn from_loss(loss: f64) -> Self {
        if loss < 0.2 {
            Self::Excellent
        } else if loss < 0.6 {
            Self::Great
        } else if loss < 1.2 {
            Self::Good
        } else if loss < 4.0 {
            Self::Inaccuracy
        } else if loss < 10.0 {
            Self::Mistake
        } else {
            Self::Blunder
        }
    }
    pub fn is_good(self) -> bool {
        matches!(self, Self::Excellent | Self::Great | Self::Good)
    }
    pub fn is_mistake(self) -> bool {
        matches!(self, Self::Mistake | Self::Blunder)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Sample {
    pub move_number: u32,
    pub is_black: bool,
    pub coordinate: String,
    pub winrate_loss: f64,
    pub score_loss: Option<f64>,
    pub first_choice: bool,
    /// Zero-based candidate position; 999 means absent.
    pub ai_rank: u32,
    pub category: MoveCategory,
    pub move_rank: u8,
    pub score_equivalent_loss: f64,
    pub complexity: f64,
    pub adjusted_weight: f64,
}

impl Sample {
    pub fn from_input(input: &MoveInput) -> Option<Self> {
        if input.move_number == 0
            || input.coordinate.trim().is_empty()
            || !input.winrate_loss.is_finite()
            || input.score_loss.is_some_and(|v| !v.is_finite())
            || input.move_rank > 5
        {
            return None;
        }
        let found = input
            .candidates
            .iter()
            .position(|c| c.coordinate.eq_ignore_ascii_case(&input.coordinate));
        let pair = input
            .candidates
            .first()
            .zip(found.and_then(|i| input.candidates.get(i)));
        let candidate_score = pair.and_then(|(top, actual)| score_difference(top, actual));
        let candidate_winrate = pair.and_then(|(top, actual)| winrate_difference(top, actual));
        let candidate_payload = candidate_score.is_some() || candidate_winrate.is_some();
        let score_loss = if candidate_payload {
            candidate_score
        } else {
            input.score_loss
        };
        let winrate_loss = candidate_winrate.unwrap_or(input.winrate_loss).max(0.0);
        let score_loss = score_loss.map(|v| v.max(0.0));
        let score_equivalent_loss = score_loss.unwrap_or(winrate_loss / 6.0);
        let complexity = complexity(&input.candidates);
        let adjusted_weight = complexity.max(score_equivalent_loss / 4.0).clamp(0.05, 1.0);
        if ![
            winrate_loss,
            score_equivalent_loss,
            complexity,
            adjusted_weight,
        ]
        .iter()
        .all(|v| v.is_finite())
            || score_loss.is_some_and(|v| !v.is_finite())
        {
            return None;
        }
        Some(Self {
            move_number: input.move_number,
            is_black: input.is_black,
            coordinate: input.coordinate.clone(),
            winrate_loss,
            score_loss,
            first_choice: found == Some(0),
            ai_rank: found.map_or(999, |i| i as u32),
            category: MoveCategory::from_loss(score_equivalent_loss),
            move_rank: input.move_rank,
            score_equivalent_loss,
            complexity,
            adjusted_weight,
        })
    }
}

fn score_difference(top: &CandidateInput, actual: &CandidateInput) -> Option<f64> {
    let value = top.score_lead? - actual.score_lead?;
    value.is_finite().then_some(value)
}
fn winrate_difference(top: &CandidateInput, actual: &CandidateInput) -> Option<f64> {
    let value = top.winrate - actual.winrate;
    value.is_finite().then_some(value)
}
fn equivalent_difference(top: &CandidateInput, actual: &CandidateInput) -> Option<f64> {
    score_difference(top, actual).or_else(|| winrate_difference(top, actual).map(|v| v / 6.0))
}
fn complexity(candidates: &[CandidateInput]) -> f64 {
    let Some(top) = candidates.first() else {
        return 0.0;
    };
    let mut loss_sum = 0.0;
    let mut prior_sum = 0.0;
    for (index, candidate) in candidates.iter().enumerate() {
        let order = if candidate.order > 0 {
            candidate.order as usize
        } else {
            index
        };
        if order >= 999 || !candidate.prior.is_finite() || candidate.prior <= 0.0 {
            continue;
        }
        if let Some(loss) = equivalent_difference(top, candidate) {
            loss_sum += loss.max(0.0) * candidate.prior;
            prior_sum += candidate.prior;
        }
    }
    if !loss_sum.is_finite() || !prior_sum.is_finite() {
        return f64::NAN;
    }
    if prior_sum > 0.0 {
        (loss_sum / prior_sum).clamp(0.0, 1.0)
    } else {
        candidates
            .get(1)
            .and_then(|c| equivalent_difference(top, c))
            .map_or(0.0, |v| (v.max(0.0) / 1.2).clamp(0.0, 1.0))
    }
}
