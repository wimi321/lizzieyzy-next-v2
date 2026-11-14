use crate::{DesktopState, error::CommandError};
use app_core::{ColorView, MatchSetup, MatchSide, MatchView};
use tauri::State;

#[tauri::command]
pub async fn start_match(
    state: State<'_, DesktopState>,
    setup: MatchSetup,
) -> Result<MatchView, CommandError> {
    Ok(state.core.start_match(setup).await?)
}

#[tauri::command]
pub async fn pause_match(state: State<'_, DesktopState>) -> Result<MatchView, CommandError> {
    Ok(state.core.pause_match().await?)
}

#[tauri::command]
pub async fn resume_match(state: State<'_, DesktopState>) -> Result<MatchView, CommandError> {
    Ok(state.core.resume_match().await?)
}

#[tauri::command]
pub async fn stop_match(state: State<'_, DesktopState>) -> Result<MatchView, CommandError> {
    Ok(state.core.stop_match().await?)
}

#[tauri::command]
pub async fn set_match_games(
    state: State<'_, DesktopState>,
    n: u32,
) -> Result<MatchView, CommandError> {
    Ok(state.core.set_match_games(n).await?)
}

#[tauri::command]
pub async fn match_play_now(state: State<'_, DesktopState>) -> Result<MatchView, CommandError> {
    Ok(state.core.match_play_now().await?)
}

#[tauri::command]
pub async fn match_resign(
    state: State<'_, DesktopState>,
    color: ColorView,
) -> Result<MatchView, CommandError> {
    Ok(state.core.match_resign(color.into()).await?)
}

#[tauri::command]
pub async fn set_match_side_params(
    state: State<'_, DesktopState>,
    side: MatchSide,
    wrn: f64,
    pda: f64,
) -> Result<MatchView, CommandError> {
    Ok(state.core.set_match_side_params(side, wrn, pda).await?)
}

#[tauri::command]
pub async fn match_view(state: State<'_, DesktopState>) -> Result<Option<MatchView>, CommandError> {
    Ok(state.core.match_view().await?)
}
