//! Capture IPC only adapts transport and delegates to the core/native lifecycle.
use app_core::{CapturePoint, CaptureRecognition, CaptureResult, CaptureSettings};
use tauri::{State, ipc::Response};

use crate::{
    DesktopState,
    bindings::Revision,
    capture::{self, CapturePermission, ScreenCapture, ScreenCaptureStarted},
    error::CommandError,
};

#[tauri::command]
pub fn capture_permission() -> CapturePermission {
    capture::permission()
}

#[tauri::command]
pub fn request_capture_permission() -> CapturePermission {
    capture::request_permission()
}

#[tauri::command]
pub async fn open_capture_permission_settings() -> Result<(), CommandError> {
    tokio::task::spawn_blocking(capture::open_permission_settings)
        .await
        .map_err(|error| capture::ScreenCaptureError::Failed {
            reason: error.to_string(),
        })??;
    Ok(())
}

#[tauri::command]
pub async fn start_screen_capture(
    service: State<'_, ScreenCapture>,
) -> Result<ScreenCaptureStarted, CommandError> {
    service.start().await
}

#[tauri::command]
pub async fn capture_image(
    id: u32,
    state: State<'_, DesktopState>,
) -> Result<Response, CommandError> {
    let image = state.core.capture_image(id).await?;
    let bytes = tokio::task::spawn_blocking(move || capture::png_bytes(&image))
        .await
        .map_err(|error| capture::ScreenCaptureError::Failed {
            reason: error.to_string(),
        })??;
    // Encoding runs outside the actor: a cancelled/replaced capture must not
    // leak a late screenshot back into a new capture page.
    state.core.capture_image(id).await?;
    Ok(Response::new(bytes))
}

#[tauri::command]
pub async fn recognize_capture(
    id: u32,
    points: [CapturePoint; 3],
    state: State<'_, DesktopState>,
) -> Result<CaptureRecognition, CommandError> {
    Ok(state.core.recognize_capture(id, points).await?)
}

#[tauri::command]
pub async fn finish_capture(
    id: u32,
    result: CaptureResult,
    service: State<'_, ScreenCapture>,
) -> Result<Revision, CommandError> {
    service.finish(id, result).await
}

#[tauri::command]
pub async fn cancel_capture(
    id: u32,
    service: State<'_, ScreenCapture>,
) -> Result<(), CommandError> {
    service.cancel(id).await
}

#[tauri::command]
pub async fn capture_settings(
    state: State<'_, DesktopState>,
) -> Result<CaptureSettings, CommandError> {
    Ok(state.core.capture_settings().await?)
}

#[tauri::command]
pub async fn set_capture_settings(
    settings: CaptureSettings,
    state: State<'_, DesktopState>,
) -> Result<CaptureSettings, CommandError> {
    Ok(state.core.set_capture_settings(settings).await?)
}
