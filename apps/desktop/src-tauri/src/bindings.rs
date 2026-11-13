//! Export the IPC DTOs from Rust. Run `cargo run -p lizzieyzy-desktop --bin export-bindings`.

use std::{fs, path::Path};

use app_core::fox::{FoxPage, FoxQuery, FoxSearchResult};
use app_core::{
    AppEvent, EngineSetupProfile, MoveView, ProblemMovesQuery, ProblemMovesView, Settings,
    StrengthEstimateView, TsumegoOptions, TsumegoStatus, VariationPreviewView, VariationTreeView,
    WholeGameOptions, WholeGamePreset, engine_setup::EngineDiscovery,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
pub struct Revision(#[ts(type = "number")] pub u64);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
pub struct SubscriptionId(pub u32);

/// Export roots recursively so input-only types are covered as well as events.
/// The destination must be a dedicated directory for generated bindings.
pub fn export(directory: &Path) -> Result<(), ts_rs::ExportError> {
    fs::create_dir_all(directory)?;
    AppEvent::export_all_to(directory)?;
    app_core::FullLogStatus::export_all_to(directory)?;
    app_core::UpdateCheck::export_all_to(directory)?;
    app_core::BoardSyncOptions::export_all_to(directory)?;
    MoveView::export_all_to(directory)?;
    app_core::NewGameOptions::export_all_to(directory)?;
    app_core::GameInfoEdit::export_all_to(directory)?;
    VariationTreeView::export_all_to(directory)?;
    VariationPreviewView::export_all_to(directory)?;
    app_core::StoneOriginView::export_all_to(directory)?;
    Settings::export_all_to(directory)?;
    app_core::BoardTransform::export_all_to(directory)?;
    app_core::JavaImportPreview::export_all_to(directory)?;
    app_core::OfficialWeight::export_all_to(directory)?;
    app_core::AccelerationPlan::export_all_to(directory)?;
    app_core::EngineDiagnostics::export_all_to(directory)?;
    app_core::DiagnosticRepair::export_all_to(directory)?;
    EngineSetupProfile::export_all_to(directory)?;
    app_core::EngineDisplayName::export_all_to(directory)?;
    EngineDiscovery::export_all_to(directory)?;
    FoxQuery::export_all_to(directory)?;
    FoxSearchResult::export_all_to(directory)?;
    FoxPage::export_all_to(directory)?;
    crate::error::CommandError::export_all_to(directory)?;
    WholeGameOptions::export_all_to(directory)?;
    WholeGamePreset::export_all_to(directory)?;
    ProblemMovesQuery::export_all_to(directory)?;
    ProblemMovesView::export_all_to(directory)?;
    StrengthEstimateView::export_all_to(directory)?;
    TsumegoOptions::export_all_to(directory)?;
    TsumegoStatus::export_all_to(directory)?;
    Revision::export_all_to(directory)?;
    SubscriptionId::export_all_to(directory)?;
    Ok(())
}

#[cfg(test)]
mod tests;
