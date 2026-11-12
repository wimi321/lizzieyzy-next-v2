use tauri::State;

use crate::{DesktopState, bindings::Revision, error::CommandError};

#[tauri::command]
pub async fn cache_enabled(state: State<'_, DesktopState>) -> Result<bool, CommandError> {
    Ok(state.core.cache_enabled().await?)
}

#[tauri::command]
pub async fn set_cache_enabled(
    state: State<'_, DesktopState>,
    enabled: bool,
) -> Result<bool, CommandError> {
    Ok(state.core.set_cache_enabled(enabled).await?)
}

#[tauri::command]
pub async fn clear_cache(state: State<'_, DesktopState>, all: bool) -> Result<(), CommandError> {
    Ok(state.core.clear_cache(all).await?)
}

#[tauri::command]
pub async fn clear_analysis(
    state: State<'_, DesktopState>,
    all: bool,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.clear_analysis(all).await?))
}
