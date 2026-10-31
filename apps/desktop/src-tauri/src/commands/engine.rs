use app_core::{EngineProfile, EngineSetupProfile, engine_setup::EngineDiscovery};
use tauri::State;

use crate::{DesktopState, engine_setup::SetupEnvironment, error::CommandError};

#[tauri::command]
pub async fn configure_engine(
    state: State<'_, DesktopState>,
    profile: Option<EngineProfile>,
) -> Result<(), CommandError> {
    Ok(state.core.configure_engine(profile).await?)
}

#[tauri::command]
pub async fn detect_engines(
    environment: State<'_, SetupEnvironment>,
) -> Result<EngineDiscovery, CommandError> {
    let roots = environment.roots.clone();
    tokio::task::spawn_blocking(move || app_core::engine_setup::detect(&roots))
        .await
        .map_err(|error| CommandError::App(format!("engine detection failed: {error}")))
}

#[tauri::command]
pub async fn auto_configure_engine(
    state: State<'_, DesktopState>,
    environment: State<'_, SetupEnvironment>,
) -> Result<EngineProfile, CommandError> {
    Ok(state
        .core
        .auto_configure_engine(
            environment.roots.clone(),
            environment.default_config_source.clone(),
        )
        .await?)
}

#[tauri::command]
pub async fn try_configure_engine(
    state: State<'_, DesktopState>,
    environment: State<'_, SetupEnvironment>,
    profile: EngineSetupProfile,
) -> Result<(), CommandError> {
    Ok(state
        .core
        .try_configure_engine(profile, environment.default_config_source.clone())
        .await?)
}
