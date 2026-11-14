//! Human-play commands are thin adapters over the single application actor.
use crate::{DesktopState, bindings::Revision, error::CommandError};
use app_core::{AiPlayer, ColorView, GameSetup, GameView, PvPick};
use tauri::State;

#[tauri::command]
pub async fn start_game(
    state: State<'_, DesktopState>,
    setup: GameSetup,
) -> Result<GameView, CommandError> {
    Ok(state.core.start_game(setup).await?)
}

#[tauri::command]
pub async fn pause_game(state: State<'_, DesktopState>) -> Result<GameView, CommandError> {
    Ok(state.core.pause_game().await?)
}

#[tauri::command]
pub async fn resume_game(state: State<'_, DesktopState>) -> Result<GameView, CommandError> {
    Ok(state.core.resume_game().await?)
}

#[tauri::command]
pub async fn stop_game(state: State<'_, DesktopState>) -> Result<GameView, CommandError> {
    Ok(state.core.stop_game().await?)
}

#[tauri::command]
pub async fn resign_game(state: State<'_, DesktopState>) -> Result<GameView, CommandError> {
    Ok(state.core.resign_game().await?)
}

#[tauri::command]
pub async fn take_back(state: State<'_, DesktopState>) -> Result<GameView, CommandError> {
    Ok(state.core.take_back().await?)
}

#[tauri::command]
pub async fn continue_game(
    state: State<'_, DesktopState>,
    ai_color: Option<ColorView>,
) -> Result<GameView, CommandError> {
    Ok(state.core.continue_game(ai_color.map(Into::into)).await?)
}

#[tauri::command]
pub async fn set_ai_player(
    state: State<'_, DesktopState>,
    color: ColorView,
    player: AiPlayer,
) -> Result<Option<GameView>, CommandError> {
    Ok(state.core.set_ai_player(color.into(), player).await?)
}

#[tauri::command]
pub async fn play_best_move(
    state: State<'_, DesktopState>,
    pv: Option<PvPick>,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.play_best_move(pv).await?))
}

#[tauri::command]
pub async fn game_view(state: State<'_, DesktopState>) -> Result<Option<GameView>, CommandError> {
    Ok(state.core.game_view().await?)
}
