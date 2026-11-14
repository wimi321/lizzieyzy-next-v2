use app_core::tencent::{TencentPage, TencentQuery};
use tauri::State;

use crate::{DesktopState, bindings::Revision, error::CommandError};

#[tauri::command]
pub async fn tencent_search(
    state: State<'_, DesktopState>,
    query: TencentQuery,
) -> Result<TencentPage, CommandError> {
    Ok(state.core.tencent_search(query).await?)
}

#[tauri::command]
pub async fn tencent_next_page(
    state: State<'_, DesktopState>,
    query: TencentQuery,
    cursor: String,
) -> Result<TencentPage, CommandError> {
    Ok(state.core.tencent_next_page(query, cursor).await?)
}

#[tauri::command]
pub async fn tencent_open_game(
    state: State<'_, DesktopState>,
    chessid: String,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.tencent_open_game(chessid).await?))
}

#[tauri::command]
pub async fn tencent_clear_history(state: State<'_, DesktopState>) -> Result<(), CommandError> {
    Ok(state.core.tencent_clear_history().await?)
}

#[tauri::command]
pub async fn open_url(
    state: State<'_, DesktopState>,
    url: String,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.open_url(url).await?))
}
