use app_core::fox::{FoxPage, FoxQuery, FoxSearchResult};
use tauri::State;

use crate::{DesktopState, bindings::Revision, error::CommandError};

#[tauri::command]
pub async fn fox_search(
    state: State<'_, DesktopState>,
    query: FoxQuery,
) -> Result<FoxSearchResult, CommandError> {
    Ok(state.core.fox_search(query).await?)
}

#[tauri::command]
pub async fn fox_next_page(
    state: State<'_, DesktopState>,
    uid: String,
    cursor: String,
) -> Result<FoxPage, CommandError> {
    Ok(state.core.fox_next_page(uid, cursor).await?)
}

#[tauri::command]
pub async fn fox_open_game(
    state: State<'_, DesktopState>,
    game_id: String,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.fox_open_game(game_id).await?))
}

#[tauri::command]
pub async fn fox_clear_history(state: State<'_, DesktopState>) -> Result<(), CommandError> {
    Ok(state.core.fox_clear_history().await?)
}
