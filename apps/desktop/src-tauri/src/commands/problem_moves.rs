//! Read-only problem-move views and persisted classification settings.
use app_core::{MoveRankSettings, ProblemMovesQuery, ProblemMovesView};
use tauri::State;

use crate::{DesktopState, error::CommandError};

#[tauri::command]
pub async fn problem_moves(
    state: State<'_, DesktopState>,
    query: ProblemMovesQuery,
) -> Result<ProblemMovesView, CommandError> {
    Ok(state.core.problem_moves(query).await?)
}

#[tauri::command]
pub async fn set_move_rank_settings(
    state: State<'_, DesktopState>,
    settings: MoveRankSettings,
) -> Result<MoveRankSettings, CommandError> {
    Ok(state.core.set_move_rank_settings(settings).await?)
}
