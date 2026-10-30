use app_core::MoveView;
use std::path::PathBuf;
use tauri::State;

use crate::{DesktopState, bindings::Revision, error::CommandError};

#[tauri::command]
pub async fn open_bytes(
    state: State<'_, DesktopState>,
    bytes: Vec<u8>,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.open_bytes(bytes).await?))
}

#[tauri::command]
pub async fn open_path(
    state: State<'_, DesktopState>,
    path: PathBuf,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.open_path(path).await?))
}

#[tauri::command]
pub async fn save(state: State<'_, DesktopState>, path: PathBuf) -> Result<(), CommandError> {
    Ok(state.core.save(path).await?)
}

#[tauri::command]
pub async fn play(state: State<'_, DesktopState>, mv: MoveView) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.play(mv.into()).await?))
}

#[tauri::command]
pub async fn step(state: State<'_, DesktopState>, count: i32) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.step(count).await?))
}

#[tauri::command]
pub async fn go_to(state: State<'_, DesktopState>, node_id: u32) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.go_to(node_id).await?))
}

#[tauri::command]
pub async fn undo(state: State<'_, DesktopState>) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.undo().await?))
}

#[tauri::command]
pub async fn redo(state: State<'_, DesktopState>) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.redo().await?))
}
