use std::path::PathBuf;

use app_core::{EngineProfile, EngineSettings, JavaImportPreview, Settings};
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn add_engine(
    state: State<'_, DesktopState>,
    name: String,
    profile: EngineProfile,
) -> Result<Settings, CommandError> {
    Ok(state.core.add_engine(name, profile).await?)
}

#[tauri::command]
pub async fn update_engine(
    state: State<'_, DesktopState>,
    id: String,
    name: String,
    profile: EngineProfile,
) -> Result<Settings, CommandError> {
    Ok(state.core.update_engine(id, name, profile).await?)
}

#[tauri::command]
pub async fn remove_engine(
    state: State<'_, DesktopState>,
    id: String,
) -> Result<Settings, CommandError> {
    Ok(state.core.remove_engine(id).await?)
}

#[tauri::command]
pub async fn reorder_engines(
    state: State<'_, DesktopState>,
    ids: Vec<String>,
) -> Result<Settings, CommandError> {
    Ok(state.core.reorder_engines(ids).await?)
}

#[tauri::command]
pub async fn select_main_engine(
    state: State<'_, DesktopState>,
    id: String,
) -> Result<Settings, CommandError> {
    Ok(state.core.select_main_engine(id).await?)
}

#[tauri::command]
pub async fn restart_engine(state: State<'_, DesktopState>) -> Result<Settings, CommandError> {
    Ok(state.core.restart_engine().await?)
}

#[tauri::command]
pub async fn close_engine(state: State<'_, DesktopState>) -> Result<Settings, CommandError> {
    Ok(state.core.close_engine().await?)
}

#[tauri::command]
pub async fn set_engine_settings(
    state: State<'_, DesktopState>,
    settings: EngineSettings,
) -> Result<EngineSettings, CommandError> {
    Ok(state.core.set_engine_settings(settings).await?)
}

#[tauri::command]
pub async fn preview_java_import(
    state: State<'_, DesktopState>,
    directory: PathBuf,
) -> Result<JavaImportPreview, CommandError> {
    Ok(state.core.preview_java_import(directory).await?)
}

#[tauri::command]
pub async fn confirm_java_import(
    state: State<'_, DesktopState>,
    preview_id: String,
) -> Result<Settings, CommandError> {
    Ok(state.core.confirm_java_import(preview_id).await?)
}
