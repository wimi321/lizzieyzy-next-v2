use app_core::{ColorView, MarkupKindView, PointView};
use tauri::State;

use crate::{DesktopState, bindings::Revision, error::CommandError};

#[tauri::command]
pub async fn set_setup_stone(
    state: State<'_, DesktopState>,
    point: PointView,
    color: Option<ColorView>,
) -> Result<Revision, CommandError> {
    Ok(Revision(
        state
            .core
            .set_setup_stone(point.into(), color.map(Into::into))
            .await?,
    ))
}

#[tauri::command]
pub async fn clear_setup(state: State<'_, DesktopState>) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.clear_setup().await?))
}

#[tauri::command]
pub async fn set_first_player(
    state: State<'_, DesktopState>,
    color: ColorView,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.set_first_player(color.into()).await?))
}

#[tauri::command]
pub async fn convert_to_start_position(
    state: State<'_, DesktopState>,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.convert_to_start_position().await?))
}

#[tauri::command]
pub async fn set_markup(
    state: State<'_, DesktopState>,
    point: PointView,
    kind: MarkupKindView,
) -> Result<Revision, CommandError> {
    Ok(Revision(
        state.core.set_markup(point.into(), kind.into()).await?,
    ))
}

#[tauri::command]
pub async fn clear_markup(state: State<'_, DesktopState>) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.clear_markup().await?))
}

#[tauri::command]
pub async fn toggle_to_play(state: State<'_, DesktopState>) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.toggle_to_play().await?))
}
