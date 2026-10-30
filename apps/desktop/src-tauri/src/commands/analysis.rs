use app_core::AppEvent;
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
