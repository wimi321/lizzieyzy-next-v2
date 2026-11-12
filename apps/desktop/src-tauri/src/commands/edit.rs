use app_core::{GameInfoEdit, NewGameOptions};
use tauri::State;

use crate::{DesktopState, bindings::Revision, error::CommandError};

#[tauri::command]
pub async fn set_comment(
    state: State<'_, DesktopState>,
    text: String,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.set_comment(text).await?))
}

#[tauri::command]
pub async fn delete_node(
    state: State<'_, DesktopState>,
    node_id: u32,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.delete_node(node_id).await?))
}

#[tauri::command]
pub async fn delete_branch(
    state: State<'_, DesktopState>,
    node_id: u32,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.delete_branch(node_id).await?))
}

#[tauri::command]
pub async fn promote_to_main(
    state: State<'_, DesktopState>,
    node_id: u32,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.promote_to_main(node_id).await?))
}

#[tauri::command]
pub async fn new_game(
    state: State<'_, DesktopState>,
    options: NewGameOptions,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.new_game(options).await?))
}

#[tauri::command]
pub async fn set_game_info(
    state: State<'_, DesktopState>,
    info: GameInfoEdit,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.set_game_info(info).await?))
}

#[tauri::command]
pub async fn recent_files(state: State<'_, DesktopState>) -> Result<Vec<String>, CommandError> {
    Ok(state.core.recent_files().await?)
}

#[tauri::command]
pub async fn clear_recent_files(state: State<'_, DesktopState>) -> Result<(), CommandError> {
    Ok(state.core.clear_recent_files().await?)
}
