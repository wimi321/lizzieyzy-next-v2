//! Batch queue adapters. AppCore owns all scheduling and file operations.
use app_core::{BatchAnalysisStatus, BatchMove, BatchOptions};
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn start_batch_analysis(
    state: State<'_, DesktopState>,
    options: BatchOptions,
) -> Result<BatchAnalysisStatus, CommandError> {
    Ok(state.core.start_batch_analysis(options).await?)
}

#[tauri::command]
pub async fn pause_batch_analysis(
    state: State<'_, DesktopState>,
) -> Result<BatchAnalysisStatus, CommandError> {
    Ok(state.core.pause_batch_analysis().await?)
}

#[tauri::command]
pub async fn resume_batch_analysis(
    state: State<'_, DesktopState>,
) -> Result<BatchAnalysisStatus, CommandError> {
    Ok(state.core.resume_batch_analysis().await?)
}

#[tauri::command]
pub async fn stop_batch_analysis(
    state: State<'_, DesktopState>,
) -> Result<BatchAnalysisStatus, CommandError> {
    Ok(state.core.stop_batch_analysis().await?)
}

#[tauri::command]
pub async fn add_batch_analysis_files(
    state: State<'_, DesktopState>,
    files: Vec<std::path::PathBuf>,
) -> Result<BatchAnalysisStatus, CommandError> {
    Ok(state.core.add_batch_analysis_files(files).await?)
}

#[tauri::command]
pub async fn remove_batch_analysis_item(
    state: State<'_, DesktopState>,
    id: u32,
) -> Result<BatchAnalysisStatus, CommandError> {
    Ok(state.core.remove_batch_analysis_item(id).await?)
}

#[tauri::command]
pub async fn move_batch_analysis_item(
    state: State<'_, DesktopState>,
    id: u32,
    direction: BatchMove,
) -> Result<BatchAnalysisStatus, CommandError> {
    Ok(state.core.move_batch_analysis_item(id, direction).await?)
}

#[tauri::command]
pub async fn clear_batch_analysis(
    state: State<'_, DesktopState>,
) -> Result<BatchAnalysisStatus, CommandError> {
    Ok(state.core.clear_batch_analysis().await?)
}

#[tauri::command]
pub async fn batch_analysis_status(
    state: State<'_, DesktopState>,
) -> Result<BatchAnalysisStatus, CommandError> {
    Ok(state.core.batch_analysis_status().await?)
}
