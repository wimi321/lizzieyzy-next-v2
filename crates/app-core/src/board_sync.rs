//! Readboard control. The application actor owns both the child and document.
use crate::{AppCore, AppError, actor};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::oneshot;
use ts_rs::TS;

pub(crate) mod process;
pub(crate) mod reconcile;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum BoardSyncLanguage {
    #[default]
    Cn,
    En,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct BoardSyncOptions {
    pub always_sync: bool,
    pub always_goto_last: bool,
    pub language: BoardSyncLanguage,
}

impl Default for BoardSyncOptions {
    fn default() -> Self {
        Self {
            always_sync: true,
            always_goto_last: false,
            language: BoardSyncLanguage::Cn,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum BoardSyncPhase {
    #[default]
    Stopped,
    Starting,
    Ready,
    Syncing,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "type", content = "data")]
pub enum BoardSyncError {
    #[error("board recognition is supported on Windows only")]
    UnsupportedPlatform,
    #[error("readboard executable is missing: {path}")]
    MissingExecutable { path: PathBuf },
    #[error("invalid board synchronization options: {reason}")]
    InvalidOptions { reason: String },
    #[error("readboard process failed: {reason}")]
    Process { reason: String },
    #[error("invalid readboard protocol: {reason}")]
    Protocol { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BoardSyncStatus {
    pub phase: BoardSyncPhase,
    pub always_sync: bool,
    pub always_goto_last: bool,
    pub remote_move_number: Option<u32>,
    pub error: Option<BoardSyncError>,
}

impl Default for BoardSyncStatus {
    fn default() -> Self {
        Self {
            phase: BoardSyncPhase::Stopped,
            always_sync: true,
            always_goto_last: false,
            remote_move_number: None,
            error: None,
        }
    }
}

pub(crate) enum Request {
    Start(PathBuf, BoardSyncOptions),
    Stop,
    Status,
}

impl AppCore {
    /// The shell resolves the installed resource; headless callers may supply
    /// another executable implementing the same stdio protocol.
    pub async fn start_board_sync(
        &self,
        program: PathBuf,
        options: BoardSyncOptions,
    ) -> Result<BoardSyncStatus, AppError> {
        self.board_sync_request(Request::Start(program, options))
            .await
    }

    /// Completes only after the child has been reaped. Idempotent.
    pub async fn stop_board_sync(&self) -> Result<BoardSyncStatus, AppError> {
        self.board_sync_request(Request::Stop).await
    }

    pub async fn board_sync_status(&self) -> Result<BoardSyncStatus, AppError> {
        self.board_sync_request(Request::Status).await
    }

    async fn board_sync_request(&self, request: Request) -> Result<BoardSyncStatus, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::BoardSync(request, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}
