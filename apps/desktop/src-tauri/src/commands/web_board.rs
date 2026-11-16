use app_core::{WebBoardOptions, WebBoardStatus};
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn start_web_board(
    state: State<'_, DesktopState>,
) -> Result<WebBoardStatus, CommandError> {
    Ok(state.core.start_web_board().await?)
}

#[tauri::command]
pub async fn stop_web_board(state: State<'_, DesktopState>) -> Result<(), CommandError> {
    Ok(state.core.stop_web_board().await?)
}

#[tauri::command]
pub async fn web_board_status(
    state: State<'_, DesktopState>,
) -> Result<WebBoardStatus, CommandError> {
    Ok(state.core.web_board_status().await?)
}

#[tauri::command]
pub async fn set_web_board_options(
    state: State<'_, DesktopState>,
    options: WebBoardOptions,
) -> Result<(), CommandError> {
    Ok(state.core.set_web_board_options(options).await?)
}

#[tauri::command]
pub async fn force_end_web_trial(state: State<'_, DesktopState>) -> Result<(), CommandError> {
    Ok(state.core.force_end_web_trial().await?)
}
