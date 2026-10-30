//! Whole-game review adapters. Scheduling, validation and storage live in AppCore.
use app_core::{AnalysisView, WholeGameOptions, WholeGameProgress};
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn start_whole_game_analysis(
    state: State<'_, DesktopState>,
    options: WholeGameOptions,
) -> Result<WholeGameProgress, CommandError> {
    Ok(state.core.start_whole_game_analysis(options).await?)
}

#[tauri::command]
pub async fn pause_whole_game_analysis(
    state: State<'_, DesktopState>,
) -> Result<WholeGameProgress, CommandError> {
    Ok(state.core.pause_whole_game_analysis().await?)
}

#[tauri::command]
pub async fn resume_whole_game_analysis(
    state: State<'_, DesktopState>,
) -> Result<WholeGameProgress, CommandError> {
    Ok(state.core.resume_whole_game_analysis().await?)
}

#[tauri::command]
pub async fn cancel_whole_game_analysis(
    state: State<'_, DesktopState>,
) -> Result<WholeGameProgress, CommandError> {
    Ok(state.core.cancel_whole_game_analysis().await?)
}

#[tauri::command]
pub async fn whole_game_progress(
    state: State<'_, DesktopState>,
) -> Result<Option<WholeGameProgress>, CommandError> {
    Ok(state.core.whole_game_progress().await?)
}

#[tauri::command]
pub async fn whole_game_result(
    state: State<'_, DesktopState>,
    node_id: u32,
) -> Result<Option<AnalysisView>, CommandError> {
    Ok(state.core.whole_game_result(node_id).await?)
}
