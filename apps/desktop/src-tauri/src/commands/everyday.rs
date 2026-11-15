//! File/clipboard adapters delegate serialization, validation and edits to Rust core.
use app_core::BoardTransform;
use tauri::State;
use tauri::ipc::Request;

use crate::{DesktopState, bindings::Revision, error::CommandError};

#[tauri::command]
pub async fn copy_sgf(state: State<'_, DesktopState>) -> Result<(), CommandError> {
    Ok(state.core.copy_sgf().await?)
}

#[tauri::command]
pub async fn paste_sgf(state: State<'_, DesktopState>) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.paste_sgf().await?))
}

#[tauri::command]
pub async fn clipboard_link(
    state: State<'_, DesktopState>,
) -> Result<Option<String>, CommandError> {
    Ok(state.core.clipboard_link().await?)
}

#[tauri::command]
pub async fn save_clean_sgf(
    state: State<'_, DesktopState>,
    path: String,
    keep_comments: bool,
    current_branch: bool,
) -> Result<(), CommandError> {
    Ok(state
        .core
        .save_clean_sgf(path, keep_comments, current_branch)
        .await?)
}

#[tauri::command]
pub async fn save_png(
    state: State<'_, DesktopState>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    let bytes = crate::png_request::bytes(&request)?;
    let path = crate::png_request::path(&request)?;
    Ok(state.core.save_png(path, bytes.to_vec()).await?)
}

#[tauri::command]
pub async fn copy_png(
    state: State<'_, DesktopState>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    let bytes = crate::png_request::bytes(&request)?;
    Ok(state.core.copy_png(bytes.to_vec()).await?)
}

#[tauri::command]
pub async fn transform_board(
    state: State<'_, DesktopState>,
    transform: BoardTransform,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.transform_board(transform).await?))
}
