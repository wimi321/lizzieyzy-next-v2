//! Application-owned save slots. A metadata rename commits an immutable SGF generation.

use std::{
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use document::GameDocument;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{ColorView, storage};

pub(crate) const AUTOSAVE_ID: &str = "autosave";
const MAX_SLOTS: usize = 100;
const MAX_NAME_CHARS: usize = 120;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SlotBoard {
    #[ts(as = "Option<u8>", optional)]
    pub width: u8,
    #[ts(as = "Option<u8>", optional)]
    pub height: u8,
    #[ts(as = "Option<Vec<Option<ColorView>>>", optional)]
    pub stones: Vec<Option<ColorView>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SaveSlot {
    #[ts(as = "Option<String>", optional)]
    pub id: String,
    #[ts(as = "Option<String>", optional)]
    pub name: String,
    #[ts(as = "Option<f64>", optional)]
    pub saved_at_unix_ms: u64,
    #[ts(as = "Option<Vec<usize>>", optional)]
    pub node_path: Vec<usize>,
    #[ts(as = "Option<usize>", optional)]
    pub move_number: usize,
    #[ts(optional = nullable)]
    pub board: Option<SlotBoard>,
    #[ts(as = "Option<bool>", optional)]
    pub is_autosave: bool,
    #[ts(as = "Option<bool>", optional)]
    pub corrupt: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum SaveSlotsError {
    #[error("invalid save slot ID")]
    InvalidId,
    #[error("save slot names must contain 1-120 characters and no control characters")]
    InvalidName,
    #[error("at most 100 manual save slots are allowed")]
    Capacity,
    #[error("automatic save slots cannot be renamed or manually overwritten")]
    ReadOnlyAutosave,
    #[error("save slot does not exist: {id}")]
    Missing { id: String },
    #[error("save slot is damaged: {id}")]
    Corrupt { id: String },
    #[error("cannot snapshot the current position: {reason}")]
    InvalidPosition { reason: String },
    #[error("save slot storage failed: {reason}")]
    Io { reason: String },
}

impl From<io::Error> for SaveSlotsError {
    fn from(error: io::Error) -> Self {
        Self::Io {
            reason: error.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Metadata {
    slot: SaveSlot,
    sgf_file: String,
    sgf_blake3: String,
}

#[derive(Clone)]
pub(crate) struct SlotStore {
    directory: PathBuf,
}

impl SlotStore {
    pub(crate) fn new(data_directory: PathBuf) -> Self {
        Self {
            directory: data_directory.join("saves"),
        }
    }

    /// Optional local data must never prevent opening the application or list.
    pub(crate) async fn list(&self) -> Vec<SaveSlot> {
        match self.list_result().await {
            Ok(slots) => slots,
            Err(error) => {
                tracing::warn!(%error, "save slots could not be listed");
                Vec::new()
            }
        }
    }

    /// Diagnostics enumerate IDs only, without opening SGF or metadata files.
    pub(crate) async fn count(&self) -> usize {
        match self.ids().await {
            Ok(ids) => ids.len(),
            Err(error) => {
                tracing::warn!(%error, "save slot count unavailable");
                0
            }
        }
    }

    async fn list_result(&self) -> Result<Vec<SaveSlot>, SaveSlotsError> {
        let mut slots = Vec::new();
        for id in self.ids().await? {
            let slot = match self.read(&id).await {
                Ok(metadata) => metadata.slot,
                Err(error) => {
                    tracing::warn!(%error, "damaged save slot");
                    self.damaged(&id).await
                }
            };
            slots.push(slot);
        }
        slots.sort_by(|a, b| {
            b.saved_at_unix_ms
                .cmp(&a.saved_at_unix_ms)
                .then(a.id.cmp(&b.id))
        });
        Ok(slots)
    }

    async fn ids(&self) -> Result<Vec<String>, SaveSlotsError> {
        match tokio::fs::symlink_metadata(&self.directory).await {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
            Ok(metadata) if !metadata.is_dir() => {
                return Err(io::Error::other("saves is not a directory").into());
            }
            Ok(_) => {}
        }
        let mut entries = tokio::fs::read_dir(&self.directory).await?;
        let mut ids = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if valid_id(&id) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    pub(crate) async fn create(
        &self,
        name: String,
        default_name: String,
        document: &GameDocument,
        bytes: Vec<u8>,
    ) -> Result<SaveSlot, SaveSlotsError> {
        let name = new_name(&name, &default_name)?;
        if self
            .ids()
            .await?
            .iter()
            .filter(|id| id.as_str() != AUTOSAVE_ID)
            .count()
            >= MAX_SLOTS
        {
            return Err(SaveSlotsError::Capacity);
        }
        self.prepare_root().await?;
        let root = self.directory.clone();
        let path = tokio::task::spawn_blocking(move || {
            tempfile::Builder::new()
                .prefix("slot-")
                .tempdir_in(root)
                .map(|dir| dir.keep())
        })
        .await
        .map_err(io::Error::other)??;
        let id = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(SaveSlotsError::InvalidId)?
            .to_owned();
        let result = self.save(&id, name, document, bytes).await;
        if result.is_err() {
            // This directory was just allocated by this operation and contains only its staged data.
            if let Err(error) = tokio::fs::remove_dir_all(path).await {
                tracing::warn!(%error, "failed to clean unsuccessful new slot");
            }
        }
        result
    }

    pub(crate) async fn overwrite(
        &self,
        id: &str,
        document: &GameDocument,
        bytes: Vec<u8>,
    ) -> Result<SaveSlot, SaveSlotsError> {
        if id == AUTOSAVE_ID {
            return Err(SaveSlotsError::ReadOnlyAutosave);
        }
        self.existing_directory(id).await?;
        let name = match self.read(id).await {
            Ok(metadata) => metadata.slot.name,
            Err(_) => self.damaged(id).await.name,
        };
        self.save(id, name, document, bytes).await
    }

    pub(crate) async fn autosave(
        &self,
        default_name: String,
        document: &GameDocument,
        bytes: Vec<u8>,
    ) -> Result<SaveSlot, SaveSlotsError> {
        let name = new_name("", &default_name)?;
        self.prepare_root().await?;
        let path = self.path(AUTOSAVE_ID)?;
        match tokio::fs::create_dir(&path).await {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        self.existing_directory(AUTOSAVE_ID).await?;
        self.save(AUTOSAVE_ID, name, document, bytes).await
    }

    pub(crate) async fn rename(&self, id: &str, name: String) -> Result<SaveSlot, SaveSlotsError> {
        if id == AUTOSAVE_ID {
            return Err(SaveSlotsError::ReadOnlyAutosave);
        }
        let name = checked_name(&name)?;
        let mut metadata = self.read(id).await?;
        metadata.slot.name = name;
        self.write_metadata(id, &metadata).await?;
        Ok(metadata.slot)
    }

    pub(crate) async fn load(&self, id: &str) -> Result<(PathBuf, SaveSlot), SaveSlotsError> {
        let metadata = self.read(id).await?;
        Ok((self.path(id)?.join(metadata.sgf_file), metadata.slot))
    }

    pub(crate) async fn delete(&self, id: &str) -> Result<(), SaveSlotsError> {
        let path = self.path(id)?;
        self.check_root().await?;
        let metadata = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|error| missing(id, error))?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileTypeExt;
            if metadata.file_type().is_symlink_dir() {
                tokio::fs::remove_dir(path).await?;
                return Ok(());
            }
        }
        if metadata.is_dir() {
            tokio::fs::remove_dir_all(path).await?;
        } else {
            // A damaged slot may itself be a file or link; unlink it, never follow it.
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }

    pub(crate) async fn delete_all(&self) -> Result<(), SaveSlotsError> {
        for id in self.ids().await? {
            if id != AUTOSAVE_ID {
                self.delete(&id).await?;
            }
        }
        Ok(())
    }

    async fn prepare_root(&self) -> Result<(), SaveSlotsError> {
        tokio::fs::create_dir_all(&self.directory).await?;
        self.check_root().await
    }

    async fn check_root(&self) -> Result<(), SaveSlotsError> {
        if !tokio::fs::symlink_metadata(&self.directory).await?.is_dir() {
            return Err(io::Error::other("saves is not a directory").into());
        }
        Ok(())
    }

    fn path(&self, id: &str) -> Result<PathBuf, SaveSlotsError> {
        if !valid_id(id) {
            return Err(SaveSlotsError::InvalidId);
        }
        Ok(self.directory.join(id))
    }

    async fn existing_directory(&self, id: &str) -> Result<PathBuf, SaveSlotsError> {
        let path = self.path(id)?;
        self.check_root().await?;
        if !tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|error| missing(id, error))?
            .is_dir()
        {
            return Err(corrupt(id));
        }
        Ok(path)
    }

    async fn read(&self, id: &str) -> Result<Metadata, SaveSlotsError> {
        let directory = self.existing_directory(id).await?;
        let metadata_path = directory.join("metadata.json");
        let bytes = regular_file(&metadata_path)
            .await
            .map_err(|_| corrupt(id))?;
        let metadata: Metadata = serde_json::from_slice(&bytes).map_err(|_| corrupt(id))?;
        if !valid_metadata(&metadata, id) {
            return Err(corrupt(id));
        }
        let bytes = regular_file(&directory.join(&metadata.sgf_file))
            .await
            .map_err(|_| corrupt(id))?;
        if blake3::hash(&bytes).to_hex().as_str() != metadata.sgf_blake3
            || sgf::parse(&bytes).is_err()
        {
            return Err(corrupt(id));
        }
        Ok(metadata)
    }

    async fn damaged(&self, id: &str) -> SaveSlot {
        let fallback = || SaveSlot {
            id: id.to_owned(),
            name: id.to_owned(),
            saved_at_unix_ms: 0,
            node_path: Vec::new(),
            move_number: 0,
            board: None,
            is_autosave: id == AUTOSAVE_ID,
            corrupt: true,
        };
        let Ok(directory) = self.existing_directory(id).await else {
            return fallback();
        };
        let Ok(bytes) = regular_file(&directory.join("metadata.json")).await else {
            return fallback();
        };
        let Ok(metadata) = serde_json::from_slice::<Metadata>(&bytes) else {
            return fallback();
        };
        if !valid_metadata(&metadata, id) {
            return fallback();
        }
        SaveSlot {
            corrupt: true,
            ..metadata.slot
        }
    }

    async fn save(
        &self,
        id: &str,
        name: String,
        document: &GameDocument,
        bytes: Vec<u8>,
    ) -> Result<SaveSlot, SaveSlotsError> {
        let slot = snapshot(id, name, document)?;
        let directory = self.existing_directory(id).await?;
        let digest = blake3::hash(&bytes).to_hex().to_string();
        let path = storage::write_generation(directory.clone(), bytes).await?;
        let sgf_file = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(SaveSlotsError::InvalidId)?
            .to_owned();
        let metadata = Metadata {
            slot,
            sgf_file,
            sgf_blake3: digest,
        };
        if let Err(error) = self.write_metadata(id, &metadata).await {
            if let Err(cleanup) = tokio::fs::remove_file(path).await {
                tracing::warn!(%cleanup, "failed to remove uncommitted save generation");
            }
            return Err(error);
        }
        // The JSON rename above is the commit point. Cleanup errors do not undo that commit.
        if let Err(error) = clean_generations(&directory, &metadata.sgf_file).await {
            tracing::warn!(%error, "failed to clean old save generations");
        }
        Ok(metadata.slot)
    }

    async fn write_metadata(&self, id: &str, metadata: &Metadata) -> Result<(), SaveSlotsError> {
        let bytes = serde_json::to_vec_pretty(metadata).map_err(io::Error::other)?;
        storage::write(self.path(id)?.join("metadata.json"), bytes).await?;
        Ok(())
    }
}

fn valid_id(id: &str) -> bool {
    id == AUTOSAVE_ID
        || (id.starts_with("slot-")
            && (6..=80).contains(&id.len())
            && id
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-'))
}

fn checked_name(name: &str) -> Result<String, SaveSlotsError> {
    if name.chars().any(char::is_control) {
        return Err(SaveSlotsError::InvalidName);
    }
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(SaveSlotsError::InvalidName);
    }
    Ok(name.to_owned())
}

fn new_name(name: &str, default_name: &str) -> Result<String, SaveSlotsError> {
    if name.chars().any(char::is_control) {
        return Err(SaveSlotsError::InvalidName);
    }
    if !name.trim().is_empty() {
        return checked_name(name);
    }
    let name: String = default_name
        .chars()
        .filter(|ch| !ch.is_control())
        .take(MAX_NAME_CHARS)
        .collect();
    if name.trim().is_empty() {
        Ok("LizzieYzy.sgf".to_owned())
    } else {
        checked_name(&name)
    }
}

fn snapshot(id: &str, name: String, document: &GameDocument) -> Result<SaveSlot, SaveSlotsError> {
    let invalid_position = |error: document::DocError| SaveSlotsError::InvalidPosition {
        reason: error.to_string(),
    };
    let position = document
        .position_at(document.current())
        .map_err(invalid_position)?;
    let mut current = document.current();
    let mut node_path = Vec::new();
    while let Some(parent) = document.node(current).map_err(invalid_position)?.parent() {
        let index = document
            .node(parent)
            .map_err(invalid_position)?
            .children()
            .iter()
            .position(|child| *child == current)
            .ok_or_else(|| SaveSlotsError::InvalidPosition {
                reason: "missing child in document path".into(),
            })?;
        node_path.push(index);
        current = parent;
    }
    node_path.reverse();
    Ok(SaveSlot {
        id: id.to_owned(),
        name,
        saved_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_millis()
            .min(9_007_199_254_740_991) as u64,
        node_path,
        move_number: position.moves.len(),
        board: Some(SlotBoard {
            width: position.board.size().width,
            height: position.board.size().height,
            stones: position
                .board
                .stones()
                .iter()
                .map(|stone| stone.map(ColorView::from))
                .collect(),
        }),
        is_autosave: id == AUTOSAVE_ID,
        corrupt: false,
    })
}

fn valid_metadata(metadata: &Metadata, id: &str) -> bool {
    let slot = &metadata.slot;
    slot.id == id
        && !slot.corrupt
        && slot.is_autosave == (id == AUTOSAVE_ID)
        && checked_name(&slot.name).is_ok_and(|name| name == slot.name)
        && slot.saved_at_unix_ms <= 9_007_199_254_740_991
        && slot.board.as_ref().is_some_and(|board| {
            (2..=25).contains(&board.width)
                && (2..=25).contains(&board.height)
                && board.stones.len() == usize::from(board.width) * usize::from(board.height)
        })
        && valid_generation(&metadata.sgf_file)
}

fn valid_generation(name: &str) -> bool {
    name.starts_with("game-")
        && name.ends_with(".sgf")
        && name.len() <= 80
        && name
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-' || ch == b'.')
        && !name.contains("..")
}

async fn regular_file(path: &Path) -> io::Result<Vec<u8>> {
    if !tokio::fs::symlink_metadata(path).await?.is_file() {
        return Err(io::Error::other("not a regular save file"));
    }
    tokio::fs::read(path).await
}

async fn clean_generations(directory: &Path, keep: &str) -> io::Result<()> {
    let mut entries = tokio::fs::read_dir(directory).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name != keep && valid_generation(name))
        {
            tokio::fs::remove_file(entry.path()).await?;
        }
    }
    Ok(())
}

fn corrupt(id: &str) -> SaveSlotsError {
    SaveSlotsError::Corrupt { id: id.to_owned() }
}

fn missing(id: &str, error: io::Error) -> SaveSlotsError {
    if error.kind() == io::ErrorKind::NotFound {
        SaveSlotsError::Missing { id: id.to_owned() }
    } else {
        error.into()
    }
}

#[cfg(test)]
mod tests;
