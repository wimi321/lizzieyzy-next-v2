//! Local-window adapters; updater and diagnostics state remain in AppCore.
use crate::{DesktopState, engine_setup::SetupEnvironment, error::CommandError};
use app_core::{FullLogStatus, UiLanguage, UpdateCheck, UpdateDownloadStatus};
use tauri::{Runtime, State};

#[tauri::command]
pub async fn set_language(
    state: State<'_, DesktopState>,
    language: UiLanguage,
) -> Result<UiLanguage, CommandError> {
    Ok(state.core.set_language(language).await?)
}

#[tauri::command]
pub async fn set_auto_check_updates(
    state: State<'_, DesktopState>,
    enabled: bool,
) -> Result<bool, CommandError> {
    Ok(state.core.set_auto_check_updates(enabled).await?)
}

#[tauri::command]
pub async fn check_update(state: State<'_, DesktopState>) -> Result<UpdateCheck, CommandError> {
    Ok(state.core.check_update().await?)
}

#[tauri::command]
pub async fn download_update(
    state: State<'_, DesktopState>,
) -> Result<UpdateDownloadStatus, CommandError> {
    Ok(state.core.download_update().await?)
}

#[tauri::command]
pub async fn cancel_update_download(
    state: State<'_, DesktopState>,
) -> Result<UpdateDownloadStatus, CommandError> {
    Ok(state.core.cancel_update_download().await?)
}

#[tauri::command]
pub async fn update_download_status(
    state: State<'_, DesktopState>,
) -> Result<UpdateDownloadStatus, CommandError> {
    Ok(state.core.update_download_status().await?)
}

#[tauri::command]
pub async fn install_update<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, DesktopState>,
) -> Result<UpdateDownloadStatus, CommandError> {
    let status = state.core.install_update().await?;
    // The updater actor has acknowledged completion before ExitRequested can
    // ask AppCore to shut it down. Windows' installer exits itself.
    #[cfg(not(target_os = "windows"))]
    app.request_restart();
    #[cfg(target_os = "windows")]
    let _ = app;
    Ok(status)
}

#[tauri::command]
pub async fn full_log_status(
    state: State<'_, DesktopState>,
) -> Result<FullLogStatus, CommandError> {
    Ok(state.core.full_log_status().await?)
}

#[tauri::command]
pub async fn start_full_log(state: State<'_, DesktopState>) -> Result<FullLogStatus, CommandError> {
    Ok(state.core.start_full_log().await?)
}

#[tauri::command]
pub async fn stop_full_log(state: State<'_, DesktopState>) -> Result<FullLogStatus, CommandError> {
    Ok(state.core.stop_full_log().await?)
}

#[tauri::command]
pub async fn export_diagnostics(
    state: State<'_, DesktopState>,
    environment: State<'_, SetupEnvironment>,
) -> Result<String, CommandError> {
    Ok(state
        .core
        .export_diagnostics(
            environment.repair_resources(),
            env!("CARGO_PKG_VERSION").into(),
        )
        .await?
        .to_string_lossy()
        .into_owned())
}
