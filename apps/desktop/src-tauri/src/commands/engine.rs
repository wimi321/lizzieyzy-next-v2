use app_core::EngineProfile;
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn configure_engine(
    state: State<'_, DesktopState>,
    profile: Option<EngineProfile>,
) -> Result<(), CommandError> {
    Ok(state.core.configure_engine(profile).await?)
}
