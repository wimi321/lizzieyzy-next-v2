//! Pure, synchronous Java-compatible player strength estimation from saved analysis.
pub mod model;
pub mod phases;
mod rank;
mod sample;
mod stats;

pub use model::{ModelError, XGBoostStrengthModel};
pub use phases::{PhaseEstimate, PhaseExtremes, StrengthPhasesReport};
pub use rank::{RankLocale, format_rank, scale_position, scale_tier};
pub use sample::{CandidateInput, MoveCategory, MoveInput, Sample};
pub use stats::{Confidence, PhaseReport, SideReport, StrengthReport};

/// Owns the immutable, bundled model; constructing it performs no file or network I/O.
pub struct Estimator {
    model: XGBoostStrengthModel,
}

impl Estimator {
    pub fn bundled() -> Result<Self, ModelError> {
        Ok(Self {
            model: XGBoostStrengthModel::bundled()?,
        })
    }

    pub fn estimate(&self, moves: &[MoveInput]) -> StrengthReport {
        let samples: Vec<_> = moves.iter().filter_map(Sample::from_input).collect();
        let black: Vec<_> = samples.iter().filter(|s| s.is_black).cloned().collect();
        let white: Vec<_> = samples.iter().filter(|s| !s.is_black).cloned().collect();
        // The Java accumulator merges black followed by white, preserving summation order.
        let overall: Vec<_> = black.iter().chain(&white).cloned().collect();
        StrengthReport {
            has_enough_data: overall.len() >= 6,
            black: self.summarize(&black),
            white: self.summarize(&white),
            overall: self.summarize(&overall),
        }
    }

    pub fn summarize(&self, samples: &[Sample]) -> SideReport {
        stats::summarize(samples, &self.model)
    }

    /// Re-estimates each side's original samples within the requested move ranges.
    /// The whole-game report and its fixed model-feature phases remain unchanged.
    /// Missing bounds use 60/150; supplied bounds follow Java's upward normalization.
    pub fn estimate_phases(
        &self,
        report: &StrengthReport,
        bounds: Option<[i32; 2]>,
        last_move: u32,
    ) -> StrengthPhasesReport {
        phases::split(self, report, bounds, last_move)
    }
}

#[cfg(test)]
mod tests;
