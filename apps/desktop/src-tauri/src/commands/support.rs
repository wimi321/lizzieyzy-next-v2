//! Restricted diagnostics-file adapters; path policy belongs to AppCore.
use crate::{DesktopState, error::CommandError, reveal::RevealService};
use tauri::State;

#[tauri::command]
pub async fn log_directory(state: State<'_, DesktopState>) -> Result<String, CommandError> {
    Ok(state
        .core
        .log_directory()
        .await?
        .to_string_lossy()
        .into_owned())
}

#[tauri::command]
pub(crate) async fn reveal_in_folder(
    state: State<'_, DesktopState>,
    platform: State<'_, RevealService>,
    path: String,
) -> Result<(), CommandError> {
    let canonical = state
        .core
        .resolve_reveal_path(path.into())
        .await
        .map_err(reveal_error)?;
    platform
        .reveal(canonical)
        .await
        .map_err(CommandError::RevealUnavailable)
}

fn reveal_error(error: app_core::AppError) -> CommandError {
    match error {
        app_core::AppError::Support(
            app_core::support::SupportError::RevealOutsideDataDirectory,
        ) => CommandError::RevealOutsideDataDirectory,
        error => CommandError::RevealUnavailable(error.to_string()),
    }
}
