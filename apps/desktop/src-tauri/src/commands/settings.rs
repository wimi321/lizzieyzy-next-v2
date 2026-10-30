use app_core::Settings;
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn settings(state: State<'_, DesktopState>) -> Result<Settings, CommandError> {
    Ok(state.core.settings().await?)
}
