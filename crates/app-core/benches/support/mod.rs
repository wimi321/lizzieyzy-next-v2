//! Shared by the standalone benchmark and ordinary, untimed integration tests.

mod generator;
mod report;

pub use generator::{large_tree, reviewed_game};
pub use report::{Limits, Report, Scenario, ScenarioResult};

pub const WARMUPS: usize = 1;
pub const REPETITIONS: usize = 7;

#[derive(Debug, thiserror::Error)]
pub enum SupportError {
    #[error("invalid benchmark data: {0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sgf(#[from] sgf::SgfError),
    #[error(transparent)]
    BoardSize(#[from] go_core::BoardSizeError),
    #[error(transparent)]
    Move(#[from] go_core::IllegalMove),
    #[error(transparent)]
    Coordinate(#[from] go_core::CoordinateError),
    #[error(transparent)]
    Analysis(#[from] document::lz::LzError),
}
