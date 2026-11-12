//! Read-only Java configuration projection. Confirmation consumes an actor-owned preview.

use crate::{AppCore, AppError, EngineEntry, Settings, SettingsError, actor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::{io::AsyncReadExt, sync::oneshot};
use ts_rs::TS;

mod display;
mod engines;
mod options;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct JavaImportedEngine {
    #[ts(as = "Option<u32>", optional)]
    pub source_index: u32,
    #[ts(as = "Option<EngineEntry>", optional)]
    pub entry: EngineEntry,
    #[ts(as = "Option<Vec<String>>", optional)]
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct JavaImportPreview {
    #[ts(as = "Option<String>", optional)]
    pub preview_id: String,
    #[ts(as = "Option<PathBuf>", optional)]
    pub directory: PathBuf,
    #[ts(as = "Option<Vec<JavaImportedEngine>>", optional)]
    pub engines: Vec<JavaImportedEngine>,
    #[ts(optional = nullable)]
    pub imported_main_engine_id: Option<String>,
    #[ts(as = "Option<Settings>", optional)]
    pub settings: Settings,
    #[ts(as = "Option<Vec<String>>", optional)]
    pub issues: Vec<String>,
}

pub(crate) struct PendingJavaImport {
    pub preview: JavaImportPreview,
    pub base: Settings,
}

pub(crate) enum Request {
    Preview(
        PathBuf,
        oneshot::Sender<Result<JavaImportPreview, AppError>>,
    ),
    Confirm(String, oneshot::Sender<Result<Settings, AppError>>),
}

impl AppCore {
    /// The directory contains Java's config.txt and optional persist file.
    /// No source command is executed and no source file is created or modified.
    pub async fn preview_java_import(
        &self,
        directory: PathBuf,
    ) -> Result<JavaImportPreview, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::JavaImport(Request::Preview(
                directory, reply,
            )))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    /// Only an unchanged, actor-owned preview can be committed, never client JSON.
    pub async fn confirm_java_import(&self, preview_id: String) -> Result<Settings, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::JavaImport(Request::Confirm(
                preview_id, reply,
            )))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

pub(crate) fn invalid(reason: impl Into<String>) -> AppError {
    SettingsError::InvalidProfile(format!("Java import: {}", reason.into())).into()
}

pub(crate) async fn ensure_separate_directory(
    source: &Path,
    destination: &Path,
) -> Result<(), AppError> {
    // Resolve existing ancestors without creating the settings directory. This
    // also catches a symlink to the Java directory and a new subdirectory of it.
    let source = tokio::fs::canonicalize(source).await?;
    let destination = std::path::absolute(destination)?;
    // A source config may itself be a symlink to the destination settings file.
    // Replacing that target would mutate the Java configuration through its alias.
    if let Ok(target) = tokio::fs::canonicalize(destination.join("settings.json")).await {
        for name in ["config.txt", "persist"] {
            if tokio::fs::canonicalize(source.join(name))
                .await
                .is_ok_and(|path| path == target)
            {
                return Err(invalid("Java source aliases the destination settings file"));
            }
        }
    }
    let mut ancestor = destination;
    loop {
        match tokio::fs::canonicalize(&ancestor).await {
            Ok(resolved) => {
                if resolved.starts_with(&source) {
                    return Err(invalid(
                        "settings directory must be outside the Java directory",
                    ));
                }
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !ancestor.pop() {
                    return Err(error.into());
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}

async fn read_json(path: &Path) -> Result<Value, AppError> {
    const MAX_CONFIG_BYTES: u64 = 4 * 1024 * 1024;
    let file = tokio::fs::File::open(path).await?;
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(invalid("configuration exceeds 4 MiB"));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(SettingsError::from)?;
    if !value.is_object() {
        return Err(invalid("configuration root must be an object"));
    }
    Ok(value)
}

pub(crate) async fn preview(
    directory: PathBuf,
    destination: &Path,
    base: &Settings,
    sequence: u64,
) -> Result<PendingJavaImport, AppError> {
    let directory = tokio::fs::canonicalize(directory).await?;
    ensure_separate_directory(&directory, destination).await?;
    let config = read_json(&directory.join("config.txt")).await?;
    for section in ["ui", "leelaz"] {
        if config.get(section).is_some_and(|value| !value.is_object()) {
            return Err(invalid(format!("{section} must be an object")));
        }
    }
    let mut issues = Vec::new();
    let persisted = match read_json(&directory.join("persist")).await {
        Ok(value) => value,
        Err(AppError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Value::Null,
        Err(error) => {
            issues.push(format!("persist was not imported: {error}"));
            Value::Null
        }
    };
    let ui = &config["ui"];
    let leelaz = &config["leelaz"];
    let mut candidate = base.clone();
    let imported = engines::read(&directory, leelaz, &mut issues).await?;
    let selected = engines::selected_index(ui, leelaz);
    let imported_main_engine_id = selected.and_then(|index| {
        imported
            .iter()
            .find(|entry| entry.source_index == index)
            .map(|entry| entry.entry.id.clone())
    });
    if selected.is_some() && imported_main_engine_id.is_none() {
        issues
            .push("Java selected engine index is missing; existing selection was retained".into());
    }
    for imported in &imported {
        if let Some(existing) = candidate
            .engines
            .iter()
            .find(|entry| entry.id == imported.entry.id)
        {
            if existing != &imported.entry {
                return Err(invalid(
                    "an imported engine was previously edited; remove it before importing again",
                ));
            }
        } else {
            candidate.engines.push(imported.entry.clone());
        }
    }
    if candidate.main_engine_id.is_none() {
        candidate.main_engine_id = imported_main_engine_id.clone();
    } else if candidate.main_engine_id != imported_main_engine_id
        && imported_main_engine_id.is_some()
    {
        issues.push(
            "existing main engine was retained; the Java main engine can be selected explicitly"
                .into(),
        );
    }
    issues.push("import does not start imported engines; select a main engine explicitly after reviewing its issues".into());
    display::merge(ui, leelaz, &persisted, &mut candidate.display, &mut issues)?;
    options::merge(ui, leelaz, &mut candidate.engine_settings, &mut issues)?;
    let candidate = candidate.normalized()?;
    // IDs are scoped to the actor and monotonically replaced, even for identical
    // source bytes, so an earlier preview can never confirm a newer one.
    let preview_id = format!("java-import-{sequence}");
    Ok(PendingJavaImport {
        base: base.clone(),
        preview: JavaImportPreview {
            preview_id,
            directory,
            engines: imported,
            imported_main_engine_id,
            settings: candidate,
            issues,
        },
    })
}

#[cfg(test)]
mod tests;
