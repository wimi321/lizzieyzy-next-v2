use app_core::{ComparisonView, Settings};
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn comparison_state(
    state: State<'_, DesktopState>,
) -> Result<ComparisonView, CommandError> {
    Ok(state.core.comparison_state().await?)
}

#[tauri::command]
pub async fn select_comparison_engine(
    state: State<'_, DesktopState>,
    id: Option<String>,
) -> Result<Settings, CommandError> {
    Ok(state.core.select_comparison_engine(id).await?)
}

#[tauri::command]
pub async fn restart_comparison_engine(
    state: State<'_, DesktopState>,
) -> Result<Settings, CommandError> {
    Ok(state.core.restart_comparison_engine().await?)
}

#[tauri::command]
pub async fn close_comparison_engine(
    state: State<'_, DesktopState>,
) -> Result<Settings, CommandError> {
    Ok(state.core.close_comparison_engine().await?)
}

#[tauri::command]
pub async fn close_other_engines(state: State<'_, DesktopState>) -> Result<Settings, CommandError> {
    Ok(state.core.close_other_engines().await?)
}

#[tauri::command]
pub async fn close_all_engines(state: State<'_, DesktopState>) -> Result<Settings, CommandError> {
    Ok(state.core.close_all_engines().await?)
}
