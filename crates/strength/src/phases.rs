//! Java StrengthPhases-compatible presentation phases, independent of the model's
//! fixed opening/middlegame/endgame feature ranges.
use crate::{Estimator, SideReport, StrengthReport};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const DEFAULT_BOUNDS: [u32; 2] = [60, 150];
pub const MIN_PHASE_MOVES: u32 = 10;
pub const MIN_SAMPLES: usize = 6;
pub const EVEN: f64 = 0.3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StrengthPhasesReport {
    /// Last move of opening and middlegame, after normalization.
    pub bounds: [u32; 2],
    /// Opening, middlegame, and endgame, in that order.
    pub phases: [PhaseEstimate; 3],
    pub black: PhaseExtremes,
    pub white: PhaseExtremes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PhaseEstimate {
    pub index: u8,
    pub first: u32,
    pub last: u32,
    /// False when the game ended before this phase's first move.
    pub played: bool,
    /// Full summaries retain low-confidence estimates, just as whole-game reports do.
    /// Only display a side's rank when its corresponding `*_ranked` flag is true.
    pub black: SideReport,
    pub white: SideReport,
    pub black_ranked: bool,
    pub white_ranked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PhaseExtremes {
    /// Phase indices, or null when fewer than two phases are ranked or the spread is < 0.3.
    pub best: Option<u8>,
    pub worst: Option<u8>,
    pub worth_reviewing: bool,
}

/// Java normalizes provided boundaries upwards; only missing boundaries use defaults.
/// Signed inputs allow negative values to be normalized too. Widening to u32 before
/// adding the minimum span also keeps i32::MAX inputs well-defined.
pub fn normalize(bounds: Option<[i32; 2]>) -> [u32; 2] {
    let Some([opening, middle]) = bounds else {
        return DEFAULT_BOUNDS;
    };
    let opening = opening.max(MIN_PHASE_MOVES as i32) as u32;
    let middle = (middle.max(0) as u32).max(opening + MIN_PHASE_MOVES);
    [opening, middle]
}

pub fn ranked(side: &SideReport) -> bool {
    side.sample_count >= MIN_SAMPLES && side.rank_value.is_some_and(f64::is_finite)
}

/// Compares only sufficiently sampled finite ranks, preserving the earliest phase on ties.
pub fn extremes(phases: &[PhaseEstimate], is_black: bool) -> PhaseExtremes {
    let mut best = None;
    let mut worst = None;
    let mut highest = f64::NEG_INFINITY;
    let mut lowest = f64::INFINITY;
    let mut count = 0;
    for phase in phases {
        let side = if is_black { &phase.black } else { &phase.white };
        if !ranked(side) {
            continue;
        }
        if let Some(value) = side.rank_value {
            count += 1;
            if value > highest {
                highest = value;
                best = Some(phase.index);
            }
            if value < lowest {
                lowest = value;
                worst = Some(phase.index);
            }
        }
    }
    let spread = highest - lowest;
    if count < 2 || spread < EVEN {
        best = None;
        worst = None;
    }
    PhaseExtremes {
        best,
        worst,
        worth_reviewing: best.is_some() && worst.is_some() && spread >= 2.0 * EVEN,
    }
}

pub(crate) fn split(
    estimator: &Estimator,
    report: &StrengthReport,
    bounds: Option<[i32; 2]>,
    last_move: u32,
) -> StrengthPhasesReport {
    let bounds = normalize(bounds);
    let ranges = [
        (1, bounds[0].min(last_move)),
        (bounds[0] + 1, bounds[1].min(last_move)),
        (bounds[1] + 1, last_move),
    ];
    let phases = std::array::from_fn(|index| {
        let (first, last) = ranges[index];
        let summarize = |side: &SideReport| {
            let samples: Vec<_> = side
                .samples
                .iter()
                .filter(|sample| sample.move_number >= first && sample.move_number <= last)
                .cloned()
                .collect();
            estimator.summarize(&samples)
        };
        let black = summarize(&report.black);
        let white = summarize(&report.white);
        PhaseEstimate {
            index: index as u8,
            first,
            last,
            played: last >= first,
            black_ranked: ranked(&black),
            white_ranked: ranked(&white),
            black,
            white,
        }
    });
    StrengthPhasesReport {
        bounds,
        black: extremes(&phases, true),
        white: extremes(&phases, false),
        phases,
    }
}

#[cfg(test)]
mod tests;
