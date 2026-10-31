//! Settings and commands for temporarily analyzing a surrounded life-and-death
//! position. The application actor owns the original document until exit.

use crate::{AppCore, AppError, ColorView, actor};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum TsumegoToPlay {
    #[default]
    Inherit,
    Black,
    White,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum TsumegoAttacker {
    #[default]
    Auto,
    Black,
    White,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum TsumegoKoThreat {
    #[default]
    Both,
    Attacker,
    Defender,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct TsumegoOptions {
    pub to_play: TsumegoToPlay,
    pub attacker: TsumegoAttacker,
    pub ko_threat: TsumegoKoThreat,
    /// Positive grid distance. Java's default is one intersection.
    pub wall_distance: u32,
}

impl Default for TsumegoOptions {
    fn default() -> Self {
        Self {
            to_play: TsumegoToPlay::Inherit,
            attacker: TsumegoAttacker::Auto,
            ko_threat: TsumegoKoThreat::Both,
            wall_distance: 1,
        }
    }
}

impl TsumegoOptions {
    pub fn validate(self) -> Result<(), TsumegoError> {
        if self.wall_distance == 0 {
            return Err(TsumegoError::InvalidDistance);
        }
        Ok(())
    }
}

impl From<TsumegoOptions> for go_core::tsumego::TsumegoOptions {
    fn from(options: TsumegoOptions) -> Self {
        use go_core::tsumego as core;
        Self {
            to_play: match options.to_play {
                TsumegoToPlay::Inherit => core::TsumegoToPlay::Inherit,
                TsumegoToPlay::Black => core::TsumegoToPlay::Black,
                TsumegoToPlay::White => core::TsumegoToPlay::White,
            },
            attacker: match options.attacker {
                TsumegoAttacker::Auto => core::TsumegoAttacker::Auto,
                TsumegoAttacker::Black => core::TsumegoAttacker::Black,
                TsumegoAttacker::White => core::TsumegoAttacker::White,
            },
            ko_threat: match options.ko_threat {
                TsumegoKoThreat::Both => core::TsumegoKoThreat::Both,
                TsumegoKoThreat::Attacker => core::TsumegoKoThreat::Attacker,
                TsumegoKoThreat::Defender => core::TsumegoKoThreat::Defender,
                TsumegoKoThreat::None => core::TsumegoKoThreat::None,
            },
            wall_distance: options.wall_distance,
        }
    }
}

/// Read after subscribing to DocumentChanged, and refresh when that event is
/// delivered. The revision identifies the current document, including edits
/// within tsumego mode. Options are always present, even before first use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TsumegoStatus {
    #[ts(type = "number")]
    pub revision: u64,
    pub active: bool,
    /// Node in the saved original document; not a node in the generated tree.
    pub source_node: Option<u32>,
    pub options: TsumegoOptions,
    pub attacker: Option<ColorView>,
    /// Colors for which Java could not fit the requested ko threats.
    pub ko_warnings: Vec<ColorView>,
}

#[derive(Debug, thiserror::Error)]
pub enum TsumegoError {
    #[error("tsumego wall distance must be positive")]
    InvalidDistance,
    #[error("the position leaves no space for a tsumego wall")]
    NoSpace,
    #[error(transparent)]
    Generation(#[from] go_core::tsumego::TsumegoError),
}

impl AppCore {
    /// Generate from the saved original position when already in tsumego mode.
    /// The resulting document contains one setup root and no played moves.
    pub async fn generate_tsumego(&self, options: TsumegoOptions) -> Result<u64, AppError> {
        options.validate()?;
        self.send(actor::Input::GenerateTsumego(options)).await
    }

    /// Restore the complete original document and focus. A second exit is a no-op.
    pub async fn exit_tsumego(&self) -> Result<u64, AppError> {
        self.send(actor::Input::ExitTsumego).await
    }

    pub async fn tsumego_status(&self) -> Result<TsumegoStatus, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::TsumegoStatus(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }
}

#[cfg(test)]
mod tests;
