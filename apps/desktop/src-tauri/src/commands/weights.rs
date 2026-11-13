//! Weight management and diagnostics are owned by AppCore. IPC accepts catalog
//! IDs and repair kinds; bundled resource paths are resolved by the shell.
use app_core::{
    AccelerationPlan, AssetDownloadStatus, DiagnosticRepair, EngineDiagnostics, OfficialWeight,
    Settings,
};
use tauri::State;

use crate::{DesktopState, engine_setup::SetupEnvironment, error::CommandError};

#[tauri::command]
pub async fn official_weights(
    state: State<'_, DesktopState>,
) -> Result<Vec<OfficialWeight>, CommandError> {
    Ok(state.core.official_weights().await?)
}

#[tauri::command]
pub async fn start_asset_download(
    state: State<'_, DesktopState>,
    asset_id: String,
) -> Result<AssetDownloadStatus, CommandError> {
    Ok(state.core.start_asset_download(asset_id).await?)
}

#[tauri::command]
pub async fn cancel_asset_download(
    state: State<'_, DesktopState>,
) -> Result<AssetDownloadStatus, CommandError> {
    Ok(state.core.cancel_asset_download().await?)
}

#[tauri::command]
pub async fn asset_download_status(
    state: State<'_, DesktopState>,
) -> Result<AssetDownloadStatus, CommandError> {
    Ok(state.core.asset_download_status().await?)
}

#[tauri::command]
pub async fn acceleration_plan(
    state: State<'_, DesktopState>,
) -> Result<AccelerationPlan, CommandError> {
    Ok(state.core.acceleration_plan().await?)
}

#[tauri::command]
pub async fn use_official_weight(
    state: State<'_, DesktopState>,
    asset_id: String,
) -> Result<Settings, CommandError> {
    Ok(state.core.use_official_weight(asset_id).await?)
}

#[tauri::command]
pub async fn diagnose_engine(
    state: State<'_, DesktopState>,
    environment: State<'_, SetupEnvironment>,
) -> Result<EngineDiagnostics, CommandError> {
    Ok(state
        .core
        .diagnose_engine(environment.repair_resources())
        .await?)
}

#[tauri::command]
pub async fn repair_engine(
    state: State<'_, DesktopState>,
    environment: State<'_, SetupEnvironment>,
    repair: DiagnosticRepair,
) -> Result<EngineDiagnostics, CommandError> {
    Ok(state
        .core
        .repair_engine(repair, environment.repair_resources())
        .await?)
}
