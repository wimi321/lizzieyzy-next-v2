//! Export the IPC DTOs from Rust. Run `cargo run -p lizzieyzy-desktop --bin export-bindings`.

use std::{fs, path::Path};

use app_core::{
    AppEvent, EngineSetupProfile, MoveView, Settings, VariationPreviewView, VariationTreeView,
    engine_setup::EngineDiscovery,
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
    MoveView::export_all_to(directory)?;
    VariationTreeView::export_all_to(directory)?;
    VariationPreviewView::export_all_to(directory)?;
    Settings::export_all_to(directory)?;
    EngineSetupProfile::export_all_to(directory)?;
    EngineDiscovery::export_all_to(directory)?;
    Revision::export_all_to(directory)?;
    SubscriptionId::export_all_to(directory)?;
    Ok(())
}

#[cfg(test)]
mod tests;
