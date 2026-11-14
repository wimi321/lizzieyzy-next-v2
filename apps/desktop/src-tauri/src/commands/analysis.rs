use app_core::{AnalysisState, AppEvent};
use tauri::{State, ipc::Channel};

use crate::{DesktopState, bindings::SubscriptionId, error::CommandError};

#[tauri::command]
pub async fn set_pondering(
    state: State<'_, DesktopState>,
    enabled: bool,
) -> Result<(), CommandError> {
    Ok(state.core.set_pondering(enabled).await?)
}

#[tauri::command]
pub async fn analysis_state(state: State<'_, DesktopState>) -> Result<AnalysisState, CommandError> {
    Ok(state.core.analysis_state().await?)
}

#[tauri::command]
pub async fn toggle_pondering(
    state: State<'_, DesktopState>,
) -> Result<AnalysisState, CommandError> {
    Ok(state.core.toggle_pondering().await?)
}

#[tauri::command]
pub async fn subscribe(
    state: State<'_, DesktopState>,
    channel: Channel<AppEvent>,
) -> Result<SubscriptionId, CommandError> {
    Ok(SubscriptionId(state.events.subscribe(channel).await?))
}

#[tauri::command]
pub async fn unsubscribe(state: State<'_, DesktopState>, id: u32) -> Result<(), CommandError> {
    state.events.unsubscribe(id).await
}

#[tauri::command]
pub async fn set_analysis_extras(
    state: State<'_, DesktopState>,
    extras: app_core::AnalysisExtras,
) -> Result<(), CommandError> {
    Ok(state.core.set_analysis_extras(extras).await?)
}

#[tauri::command]
pub async fn set_pure_network(
    state: State<'_, DesktopState>,
    enabled: bool,
) -> Result<(), CommandError> {
    Ok(state.core.set_pure_network(enabled).await?)
}
