use serde::Serialize;
use ts_rs::TS;

use crate::fox_error::FoxFailure;

/// Serializable IPC failures. Domain errors are rendered only at this boundary.
#[derive(Debug, thiserror::Error, Serialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum CommandError {
    #[error("{0}")]
    Commentary(app_core::CommentaryError),
    #[error("{0}")]
    Coach(app_core::CoachError),
    #[error("{0}")]
    Gtp(app_core::GtpError),
    #[error("{0}")]
    Match(app_core::MatchError),
    #[error("game is running")]
    GameRunning,
    #[error("{0}")]
    Game(app_core::GameError),
    #[error("{0}")]
    DisplaySettings(app_core::DisplaySettingsError),
    #[error("pure network is active")]
    PureNetworkActive,
    #[error("whole-game analysis is running or paused")]
    WholeGameAnalysisActive,
    #[error("review range {first_move}..={last_move} is outside moves 1..={last_available}")]
    InvalidWholeGameRange {
        first_move: usize,
        last_move: usize,
        last_available: usize,
    },
    #[error("batch analysis is running")]
    BatchAnalysisRunning,
    #[error("{0}")]
    Batch(app_core::BatchError),
    #[error("{0}")]
    Clipboard(app_core::ClipboardError),
    #[error("{0}")]
    Png(app_core::PngError),
    #[error("{0}")]
    PngRequest(#[from] PngRequestError),
    #[error("cannot apply main window preference: {0}")]
    WindowUnavailable(String),
    #[error("this build has no update signing key configured")]
    UpdateSigningNotConfigured,
    #[error("only paths inside the application data directory may be revealed")]
    RevealOutsideDataDirectory,
    #[error("cannot reveal this path: {0}")]
    RevealUnavailable(String),
    #[error("{0}")]
    App(String),
    #[error("{0}")]
    Fox(FoxFailure),
    #[error("{0}")]
    AutoSetup(app_core::AutoSetupError),
    #[error("{0}")]
    BoardSync(app_core::BoardSyncError),
    #[error("event stream is closed")]
    StreamClosed,
    #[error("subscription identifiers are exhausted")]
    SubscriptionLimit,
    #[error("cannot send event: {0}")]
    Channel(String),
}

/// The binary PNG transport must be valid before any file or clipboard action.
#[derive(Debug, thiserror::Error, Serialize, TS)]
pub enum PngRequestError {
    #[error("PNG commands require a raw binary request body")]
    RawBodyRequired,
    #[error("PNG save request is missing its path header")]
    MissingPath,
    #[error("PNG save path must not be empty")]
    EmptyPath,
    #[error("PNG save path header is not valid percent-encoded UTF-8")]
    InvalidPathEncoding,
}

impl From<app_core::AppError> for CommandError {
    fn from(error: app_core::AppError) -> Self {
        match error {
            app_core::AppError::Commentary(error)
            | app_core::AppError::Settings(app_core::SettingsError::Commentary(error)) => {
                Self::Commentary(error)
            }
            app_core::AppError::Coach(error) => Self::Coach(error),
            app_core::AppError::Gtp(error) => Self::Gtp(error),
            app_core::AppError::Match(error) => Self::Match(error),
            app_core::AppError::GameRunning => Self::GameRunning,
            app_core::AppError::Game(error)
            | app_core::AppError::Settings(app_core::SettingsError::Game(error)) => {
                Self::Game(error)
            }
            app_core::AppError::Settings(app_core::SettingsError::Display(error)) => {
                Self::DisplaySettings(error)
            }
            app_core::AppError::PureNetworkActive => Self::PureNetworkActive,
            app_core::AppError::WholeGameAnalysisActive => Self::WholeGameAnalysisActive,
            app_core::AppError::WholeGame(app_core::WholeGameError::InvalidRange {
                first_move,
                last_move,
                last_available,
            }) => Self::InvalidWholeGameRange {
                first_move,
                last_move,
                last_available,
            },
            app_core::AppError::BatchAnalysisRunning => Self::BatchAnalysisRunning,
            app_core::AppError::Batch(error) => Self::Batch(error),
            app_core::AppError::Clipboard(error) => Self::Clipboard(error),
            app_core::AppError::Png(error) => Self::Png(error),
            app_core::AppError::Update(app_core::updates::UpdateError::SigningNotConfigured) => {
                Self::UpdateSigningNotConfigured
            }
            app_core::AppError::Support(
                app_core::support::SupportError::RevealOutsideDataDirectory,
            ) => Self::RevealOutsideDataDirectory,
            app_core::AppError::Fox(error) => Self::Fox(error.into()),
            app_core::AppError::AutoSetup(error) => Self::AutoSetup(error),
            app_core::AppError::BoardSync(error) => Self::BoardSync(error),
            other => Self::App(other.to_string()),
        }
    }
}
