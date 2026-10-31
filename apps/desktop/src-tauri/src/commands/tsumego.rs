//! Tsumego adapters; generation, persistence and restoration belong to AppCore.
use app_core::{TsumegoOptions, TsumegoStatus};
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn generate_tsumego(
    state: State<'_, DesktopState>,
    options: TsumegoOptions,
) -> Result<u64, CommandError> {
    Ok(state.core.generate_tsumego(options).await?)
}

#[tauri::command]
pub async fn exit_tsumego(state: State<'_, DesktopState>) -> Result<u64, CommandError> {
    Ok(state.core.exit_tsumego().await?)
}

#[tauri::command]
pub async fn tsumego_status(state: State<'_, DesktopState>) -> Result<TsumegoStatus, CommandError> {
    Ok(state.core.tsumego_status().await?)
}
