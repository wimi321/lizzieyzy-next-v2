use crate::{DesktopState, error::CommandError};
use app_core::{GtpConsoleLine, GtpEngineEntry, GtpEngineInfo, Settings};
use tauri::State;

#[tauri::command]
pub async fn add_gtp_engine(
    state: State<'_, DesktopState>,
    entry: GtpEngineEntry,
) -> Result<Settings, CommandError> {
    Ok(state.core.add_gtp_engine(entry).await?)
}

#[tauri::command]
pub async fn update_gtp_engine(
    state: State<'_, DesktopState>,
    entry: GtpEngineEntry,
) -> Result<Settings, CommandError> {
    Ok(state.core.update_gtp_engine(entry).await?)
}

#[tauri::command]
pub async fn remove_gtp_engine(
    state: State<'_, DesktopState>,
    id: String,
) -> Result<Settings, CommandError> {
    Ok(state.core.remove_gtp_engine(id).await?)
}

#[tauri::command]
pub async fn test_gtp_engine(
    state: State<'_, DesktopState>,
    id: String,
) -> Result<GtpEngineInfo, CommandError> {
    Ok(state.core.test_gtp_engine(id).await?)
}

#[tauri::command]
pub async fn gtp_console(
    state: State<'_, DesktopState>,
    engine_id: String,
) -> Result<Vec<GtpConsoleLine>, CommandError> {
    Ok(state.core.gtp_console(engine_id).await?)
}

#[tauri::command]
pub async fn gtp_console_send(
    state: State<'_, DesktopState>,
    engine_id: String,
    text: String,
) -> Result<String, CommandError> {
    Ok(state.core.gtp_console_send(engine_id, text).await?)
}
