//! Coaching and HumanSL model commands forward to the existing application actor.
use std::path::PathBuf;

use app_core::{
    AssetDownloadStatus, CoachDecision, CoachHint, CoachSetup, CoachView, HumanSlModelStatus,
};
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn start_coach(
    state: State<'_, DesktopState>,
    setup: CoachSetup,
) -> Result<CoachView, CommandError> {
    Ok(state.core.start_coach(setup).await?)
}

#[tauri::command]
pub async fn coach_view(state: State<'_, DesktopState>) -> Result<Option<CoachView>, CommandError> {
    Ok(state.core.coach_view().await?)
}

#[tauri::command]
pub async fn coach_hint(state: State<'_, DesktopState>) -> Result<CoachHint, CommandError> {
    Ok(state.core.coach_hint().await?)
}

#[tauri::command]
pub async fn coach_decision(
    state: State<'_, DesktopState>,
    decision: CoachDecision,
) -> Result<CoachHint, CommandError> {
    Ok(state.core.coach_decision(decision).await?)
}

#[tauri::command]
pub async fn coach_review(state: State<'_, DesktopState>) -> Result<(), CommandError> {
    Ok(state.core.coach_review().await?)
}

#[tauri::command]
pub async fn humansl_model_status(
    state: State<'_, DesktopState>,
) -> Result<HumanSlModelStatus, CommandError> {
    Ok(state.core.humansl_model_status().await?)
}

#[tauri::command]
pub async fn import_humansl_model(
    state: State<'_, DesktopState>,
    path: PathBuf,
) -> Result<HumanSlModelStatus, CommandError> {
    Ok(state.core.import_humansl_model(path).await?)
}

#[tauri::command]
pub async fn pause_asset_download(
    state: State<'_, DesktopState>,
) -> Result<AssetDownloadStatus, CommandError> {
    Ok(state.core.pause_asset_download().await?)
}
