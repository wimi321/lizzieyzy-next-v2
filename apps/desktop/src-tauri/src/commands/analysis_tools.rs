use crate::{DesktopState, error::CommandError};
use tauri::State;

#[tauri::command]
pub async fn set_analysis_restriction(
    state: State<'_, DesktopState>,
    kind: app_core::RestrictionKind,
    points: Vec<app_core::PointView>,
) -> Result<Option<app_core::AnalysisRestriction>, CommandError> {
    Ok(state.core.set_analysis_restriction(kind, points).await?)
}

#[tauri::command]
pub async fn add_restriction_points(
    state: State<'_, DesktopState>,
    kind: app_core::RestrictionKind,
    points: Vec<app_core::PointView>,
) -> Result<Option<app_core::AnalysisRestriction>, CommandError> {
    Ok(state.core.add_restriction_points(kind, points).await?)
}

#[tauri::command]
pub async fn remove_restriction_points(
    state: State<'_, DesktopState>,
    points: Vec<app_core::PointView>,
) -> Result<Option<app_core::AnalysisRestriction>, CommandError> {
    Ok(state.core.remove_restriction_points(points).await?)
}

#[tauri::command]
pub async fn clear_analysis_restriction(
    state: State<'_, DesktopState>,
) -> Result<(), CommandError> {
    Ok(state.core.clear_analysis_restriction().await?)
}

#[tauri::command]
pub async fn set_restriction_depth(
    state: State<'_, DesktopState>,
    kind: app_core::RestrictionKind,
    depth: app_core::RestrictionDepth,
) -> Result<(), CommandError> {
    Ok(state.core.set_restriction_depth(kind, depth).await?)
}

#[tauri::command]
pub async fn search_params(
    state: State<'_, DesktopState>,
) -> Result<Option<app_core::SearchParams>, CommandError> {
    Ok(state.core.search_params().await?)
}

#[tauri::command]
pub async fn set_search_params(
    state: State<'_, DesktopState>,
    params: Option<app_core::SearchParams>,
) -> Result<Option<app_core::SearchParams>, CommandError> {
    Ok(state.core.set_search_params(params).await?)
}

#[tauri::command]
pub async fn continue_ladder(
    state: State<'_, DesktopState>,
) -> Result<app_core::LadderContinuation, CommandError> {
    Ok(state.core.continue_ladder().await?)
}

#[tauri::command]
pub async fn start_scoring(
    state: State<'_, DesktopState>,
) -> Result<app_core::ScoringView, CommandError> {
    Ok(state.core.start_scoring().await?)
}

#[tauri::command]
pub async fn toggle_scoring_point(
    state: State<'_, DesktopState>,
    point: app_core::PointView,
) -> Result<app_core::ScoringView, CommandError> {
    Ok(state.core.toggle_scoring_point(point.into()).await?)
}

#[tauri::command]
pub async fn set_scoring_rule(
    state: State<'_, DesktopState>,
    rule: app_core::ScoringRuleView,
) -> Result<app_core::ScoringView, CommandError> {
    Ok(state.core.set_scoring_rule(rule).await?)
}

#[tauri::command]
pub async fn commit_scoring(state: State<'_, DesktopState>) -> Result<u64, CommandError> {
    Ok(state.core.commit_scoring().await?)
}

#[tauri::command]
pub async fn cancel_scoring(state: State<'_, DesktopState>) -> Result<(), CommandError> {
    Ok(state.core.cancel_scoring().await?)
}
