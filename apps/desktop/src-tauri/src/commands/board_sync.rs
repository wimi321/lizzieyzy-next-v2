//! Board synchronization adapters. Recognition is offered only on Windows.
use app_core::{BoardSyncError, BoardSyncOptions, BoardSyncStatus};
use tauri::State;

use crate::{DesktopState, board_sync::BoardSyncEnvironment, error::CommandError};

#[tauri::command]
pub async fn start_board_sync(
    state: State<'_, DesktopState>,
    environment: State<'_, BoardSyncEnvironment>,
    options: BoardSyncOptions,
) -> Result<BoardSyncStatus, CommandError> {
    if !cfg!(windows) {
        return Err(CommandError::BoardSync(BoardSyncError::UnsupportedPlatform));
    }
    Ok(state
        .core
        .start_board_sync(environment.program.clone(), options)
        .await?)
}

#[tauri::command]
pub async fn stop_board_sync(
    state: State<'_, DesktopState>,
) -> Result<BoardSyncStatus, CommandError> {
    Ok(state.core.stop_board_sync().await?)
}

#[tauri::command]
pub async fn board_sync_status(
    state: State<'_, DesktopState>,
) -> Result<BoardSyncStatus, CommandError> {
    Ok(state.core.board_sync_status().await?)
}
