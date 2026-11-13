//! Daily preferences are saved by AppCore before applying native effects.
use tauri::State;

use crate::{DesktopState, error::CommandError, window_preferences::WindowPreferences};

#[tauri::command]
pub async fn always_on_top(state: State<'_, DesktopState>) -> Result<bool, CommandError> {
    Ok(state.core.always_on_top().await?)
}

#[tauri::command]
pub(crate) async fn set_always_on_top(
    state: State<'_, DesktopState>,
    window: State<'_, WindowPreferences>,
    enabled: bool,
) -> Result<bool, CommandError> {
    window.set(&state.core, enabled).await
}

#[tauri::command]
pub async fn ui_scale(state: State<'_, DesktopState>) -> Result<f64, CommandError> {
    Ok(state.core.ui_scale().await?)
}

#[tauri::command]
pub async fn set_ui_scale(state: State<'_, DesktopState>, scale: f64) -> Result<f64, CommandError> {
    Ok(state.core.set_ui_scale(scale).await?)
}

#[tauri::command]
pub async fn play_sound(state: State<'_, DesktopState>) -> Result<bool, CommandError> {
    Ok(state.core.play_sound().await?)
}

#[tauri::command]
pub async fn set_play_sound(
    state: State<'_, DesktopState>,
    enabled: bool,
) -> Result<bool, CommandError> {
    Ok(state.core.set_play_sound(enabled).await?)
}
