//! Manual benchmark adapters. Process ownership and tuning stay in AppCore.
use app_core::BenchmarkStatus;
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn start_benchmark(
    state: State<'_, DesktopState>,
) -> Result<BenchmarkStatus, CommandError> {
    Ok(state.core.start_benchmark().await?)
}

#[tauri::command]
pub async fn cancel_benchmark(
    state: State<'_, DesktopState>,
) -> Result<BenchmarkStatus, CommandError> {
    Ok(state.core.cancel_benchmark().await?)
}

#[tauri::command]
pub async fn benchmark_status(
    state: State<'_, DesktopState>,
) -> Result<BenchmarkStatus, CommandError> {
    Ok(state.core.benchmark_status().await?)
}

#[tauri::command]
pub async fn reset_benchmark(
    state: State<'_, DesktopState>,
) -> Result<BenchmarkStatus, CommandError> {
    Ok(state.core.reset_benchmark().await?)
}
