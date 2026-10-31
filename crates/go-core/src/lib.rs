//! Pure Go board rules and coordinates.

mod coordinates;
mod position;
mod rules;
pub mod tsumego;
mod types;
mod zobrist;

pub use coordinates::CoordinateError;
pub use position::{IllegalMove, PlayOutcome, Position, SetupError};
pub use rules::{KoRule, RulePreset, Rules, ScoringRule, UnknownRules};
pub use types::{BoardSize, BoardSizeError, Color, Move, Point};

#[cfg(test)]
mod tests;
