use app_core::{MoveView, VariationPreviewView, VariationTreeView};
use tauri::State;

use crate::{DesktopState, error::CommandError};

/// Fetch only when DocumentView.tree.structure_revision changes. The returned
/// revision identifies the tree structure and can reject an obsolete reply.
#[tauri::command]
pub async fn variation_tree(
    state: State<'_, DesktopState>,
) -> Result<VariationTreeView, CommandError> {
    Ok(state.core.variation_tree().await?)
}

/// Preview a PV without editing the document or changing analysis focus.
#[tauri::command]
pub async fn preview_variation(
    state: State<'_, DesktopState>,
    node_id: u32,
    moves: Vec<MoveView>,
) -> Result<VariationPreviewView, CommandError> {
    Ok(state
        .core
        .preview_variation(node_id, moves.into_iter().map(Into::into).collect())
        .await?)
}
