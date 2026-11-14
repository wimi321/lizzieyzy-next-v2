//! Remote engine adapters. Connection, credential and image work stays in Rust core.
use app_core::{RemoteEngineInput, Settings};
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn add_remote_engine(
    state: State<'_, DesktopState>,
    name: String,
    entry: RemoteEngineInput,
) -> Result<Settings, CommandError> {
    Ok(state.core.add_remote_engine(name, entry).await?)
}

#[tauri::command]
pub async fn update_remote_engine(
    state: State<'_, DesktopState>,
    id: String,
    name: String,
    entry: RemoteEngineInput,
) -> Result<Settings, CommandError> {
    Ok(state.core.update_remote_engine(id, name, entry).await?)
}

#[tauri::command]
pub async fn test_remote_engine(
    state: State<'_, DesktopState>,
    entry: RemoteEngineInput,
) -> Result<String, CommandError> {
    Ok(state.core.test_remote_engine(entry).await?)
}

#[tauri::command]
pub async fn switch_to_local_engine(
    state: State<'_, DesktopState>,
) -> Result<Settings, CommandError> {
    Ok(state.core.switch_to_local_engine().await?)
}

#[tauri::command]
pub fn normalize_remote_link(text: String) -> Result<String, CommandError> {
    app_core::normalize_remote_link(&text).map_err(CommandError::RemoteLink)
}

#[tauri::command]
pub async fn read_remote_link_qr(path: std::path::PathBuf) -> Result<String, CommandError> {
    app_core::read_remote_link_qr(path)
        .await
        .map_err(CommandError::RemoteLink)
}
