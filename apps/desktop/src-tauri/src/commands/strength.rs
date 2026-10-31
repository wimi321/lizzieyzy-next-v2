//! Read-only strength estimates from the retained whole-game analysis.
use app_core::StrengthEstimateView;
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn strength_estimate(
    state: State<'_, DesktopState>,
    bounds: Option<[i32; 2]>,
) -> Result<StrengthEstimateView, CommandError> {
    Ok(state.core.strength_estimate(bounds).await?)
}
