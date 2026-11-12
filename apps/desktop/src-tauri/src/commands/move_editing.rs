use app_core::{MoveView, PointView, StoneOriginView};
use tauri::State;

use crate::{DesktopState, bindings::Revision, error::CommandError};

#[tauri::command]
pub async fn play_colored(
    state: State<'_, DesktopState>,
    mv: MoveView,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.play_colored(mv.into()).await?))
}

#[tauri::command]
pub async fn insert_move(
    state: State<'_, DesktopState>,
    mv: MoveView,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.insert_move(mv.into()).await?))
}

#[tauri::command]
pub async fn switch_stone(
    state: State<'_, DesktopState>,
    point: PointView,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.switch_stone(point.into()).await?))
}

#[tauri::command]
pub async fn delete_stone(
    state: State<'_, DesktopState>,
    point: PointView,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.delete_stone(point.into()).await?))
}

#[tauri::command]
pub async fn move_stone(
    state: State<'_, DesktopState>,
    from: PointView,
    to: PointView,
) -> Result<Revision, CommandError> {
    Ok(Revision(
        state.core.move_stone(from.into(), to.into()).await?,
    ))
}

#[tauri::command]
pub async fn add_variation(
    state: State<'_, DesktopState>,
    moves: Vec<MoveView>,
) -> Result<Revision, CommandError> {
    Ok(Revision(
        state
            .core
            .add_variation(moves.into_iter().map(Into::into).collect())
            .await?,
    ))
}

#[tauri::command]
pub async fn stone_origin(
    state: State<'_, DesktopState>,
    point: PointView,
) -> Result<Option<StoneOriginView>, CommandError> {
    Ok(state.core.stone_origin(point.into()).await?)
}

#[tauri::command]
pub async fn start_trial(state: State<'_, DesktopState>) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.start_trial().await?))
}

#[tauri::command]
pub async fn end_trial(state: State<'_, DesktopState>) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.end_trial().await?))
}
