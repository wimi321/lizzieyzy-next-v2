use tauri::State;

use crate::{DesktopState, bindings::Revision, error::CommandError};

#[tauri::command]
pub async fn save_slots(
    state: State<'_, DesktopState>,
) -> Result<Vec<app_core::SaveSlot>, CommandError> {
    Ok(state.core.save_slots().await?)
}

#[tauri::command]
pub async fn save_to_new_slot(
    state: State<'_, DesktopState>,
    name: String,
) -> Result<app_core::SaveSlot, CommandError> {
    Ok(state.core.save_to_new_slot(name).await?)
}

#[tauri::command]
pub async fn overwrite_slot(
    state: State<'_, DesktopState>,
    id: String,
) -> Result<app_core::SaveSlot, CommandError> {
    Ok(state.core.overwrite_slot(id).await?)
}

#[tauri::command]
pub async fn rename_slot(
    state: State<'_, DesktopState>,
    id: String,
    name: String,
) -> Result<app_core::SaveSlot, CommandError> {
    Ok(state.core.rename_slot(id, name).await?)
}

#[tauri::command]
pub async fn load_slot(
    state: State<'_, DesktopState>,
    id: String,
) -> Result<Revision, CommandError> {
    Ok(Revision(state.core.load_slot(id).await?))
}

#[tauri::command]
pub async fn delete_slot(state: State<'_, DesktopState>, id: String) -> Result<(), CommandError> {
    Ok(state.core.delete_slot(id).await?)
}

#[tauri::command]
pub async fn delete_all_slots(state: State<'_, DesktopState>) -> Result<(), CommandError> {
    Ok(state.core.delete_all_slots().await?)
}

#[tauri::command]
pub async fn set_autosave(
    state: State<'_, DesktopState>,
    enabled: bool,
) -> Result<bool, CommandError> {
    Ok(state.core.set_autosave(enabled).await?)
}

#[tauri::command]
pub async fn set_restore_last_game(
    state: State<'_, DesktopState>,
    enabled: bool,
) -> Result<bool, CommandError> {
    Ok(state.core.set_restore_last_game(enabled).await?)
}
