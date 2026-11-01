use app_core::{AnalysisLimitSettings, Settings};
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn settings(state: State<'_, DesktopState>) -> Result<Settings, CommandError> {
    Ok(state.core.settings().await?)
}

#[tauri::command]
pub async fn analysis_limits(
    state: State<'_, DesktopState>,
) -> Result<AnalysisLimitSettings, CommandError> {
    Ok(state.core.analysis_limits().await?)
}

#[tauri::command]
pub async fn set_analysis_limits(
    state: State<'_, DesktopState>,
    settings: AnalysisLimitSettings,
) -> Result<AnalysisLimitSettings, CommandError> {
    Ok(state.core.set_analysis_limits(settings).await?)
}

#[tauri::command]
pub async fn auto_quick_analyze_on_load(
    state: State<'_, DesktopState>,
) -> Result<bool, CommandError> {
    Ok(state.core.auto_quick_analyze_on_load().await?)
}

#[tauri::command]
pub async fn set_auto_quick_analyze_on_load(
    state: State<'_, DesktopState>,
    enabled: bool,
) -> Result<bool, CommandError> {
    Ok(state.core.set_auto_quick_analyze_on_load(enabled).await?)
}
