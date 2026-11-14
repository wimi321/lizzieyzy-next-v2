//! Commentary commands only forward to AppCore; credentials stay in Rust.
use app_core::{CommentaryKeyStatus, CommentaryRequest, CommentarySettings, CommentaryUpdated};
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn start_commentary(
    state: State<'_, DesktopState>,
    request: CommentaryRequest,
) -> Result<CommentaryUpdated, CommandError> {
    Ok(state.core.start_commentary(request).await?)
}

#[tauri::command]
pub async fn stop_commentary(
    state: State<'_, DesktopState>,
) -> Result<CommentaryUpdated, CommandError> {
    Ok(state.core.stop_commentary().await?)
}

#[tauri::command]
pub async fn commentary_status(
    state: State<'_, DesktopState>,
) -> Result<CommentaryUpdated, CommandError> {
    Ok(state.core.commentary_status().await?)
}

#[tauri::command]
pub async fn commentary_models(
    state: State<'_, DesktopState>,
) -> Result<Vec<String>, CommandError> {
    Ok(state.core.commentary_models().await?)
}

#[tauri::command]
pub async fn set_commentary_key(
    state: State<'_, DesktopState>,
    key: String,
    remember: bool,
) -> Result<CommentaryKeyStatus, CommandError> {
    Ok(state.core.set_commentary_key(key, remember).await?)
}

#[tauri::command]
pub async fn clear_commentary_key(
    state: State<'_, DesktopState>,
) -> Result<CommentaryKeyStatus, CommandError> {
    Ok(state.core.clear_commentary_key().await?)
}

#[tauri::command]
pub async fn commentary_key_status(
    state: State<'_, DesktopState>,
) -> Result<CommentaryKeyStatus, CommandError> {
    Ok(state.core.commentary_key_status().await?)
}

#[tauri::command]
pub async fn set_commentary_settings(
    state: State<'_, DesktopState>,
    settings: CommentarySettings,
) -> Result<CommentarySettings, CommandError> {
    Ok(state.core.set_commentary_settings(settings).await?)
}
