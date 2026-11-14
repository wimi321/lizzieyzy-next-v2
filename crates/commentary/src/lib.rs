//! Pure, bounded commentary evidence and verification. No I/O or model judgments.

mod board;
pub mod codec;
mod evidence;
mod prompt;
mod verifier;

pub use evidence::{Candidate, Evidence, PositionInput, SelectedPosition, build_evidence};
pub use prompt::{
    Detail, FollowUp, Language, Message, Mode, Preferences, Style, Terminology, Variations,
    build_correction, build_prompt,
};
pub use verifier::verify;

pub const MAX_PROMPT_CHARACTERS: usize = 250_000;
pub const MAX_ANSWER_CHARACTERS: usize = 100_000;

/// Errors never include user-provided text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    NoAnalysis,
    InvalidAnalysis,
    InvalidPreferences,
    InvalidCoordinate,
    PromptTooLong,
    EmptyCommentary,
    CommentaryTooLong,
    MissingCommentary,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NoAnalysis => "no analyzed candidates are available",
            Self::InvalidAnalysis => "analysis contains invalid values",
            Self::InvalidPreferences => "invalid commentary preferences",
            Self::InvalidCoordinate => "analysis contains a coordinate outside the board",
            Self::PromptTooLong => "commentary prompt exceeds 250000 characters",
            Self::EmptyCommentary => "commentary is empty",
            Self::CommentaryTooLong => "commentary exceeds 100000 characters",
            Self::MissingCommentary => "no complete commentary block is available",
        })
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests;
