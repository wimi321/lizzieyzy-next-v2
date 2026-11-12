//! File identity, save baselines and recent paths owned by the document actor.

use super::Actor;
use crate::{AppError, AppEvent, NewGameOptions, SettingsError, storage};
use document::GameDocument;
use std::path::PathBuf;

#[derive(Clone)]
pub(super) struct FileState {
    pub path: Option<PathBuf>,
    pub save_warnings: Vec<document::DocumentWarning>,
    saved: sgf::GameTree,
    checked_revision: u64,
    modified: bool,
}

impl FileState {
    pub fn new(document: &GameDocument, path: Option<PathBuf>) -> Self {
        Self {
            path,
            save_warnings: Vec::new(),
            saved: document.to_sgf(),
            checked_revision: document.content_revision(),
            modified: false,
        }
    }

    pub fn modified(&mut self, document: &GameDocument) -> bool {
        // Stable node IDs and cursor position are not file contents. Exact SGF
        // properties/child order also recognize undo back to a saved document.
        if self.checked_revision != document.content_revision() {
            self.save_warnings.clear();
            self.modified = self.saved != document.to_sgf();
            self.checked_revision = document.content_revision();
        }
        self.modified
    }
}

impl Actor {
    pub(super) async fn convert_to_start_position(&mut self) -> Result<(), AppError> {
        self.ensure_no_move_trial()?;
        let document = self.document.to_start_position()?;
        self.stop_board_sync().await?;
        self.open_auto_curve_document(document, None, false).await
    }

    pub(super) async fn new_game(&mut self, options: NewGameOptions) -> Result<(), AppError> {
        self.ensure_no_move_trial()?;
        let document = GameDocument::new_game(options.into())?;
        self.stop_board_sync().await?;
        self.open_auto_curve_document(document, None, false).await
    }

    pub(super) async fn save_document(&mut self, path: PathBuf) -> Result<(), AppError> {
        let path = std::path::absolute(path)?;
        if super::game_files::is_gib(&path) {
            return Err(AppError::GibSaveTarget);
        }
        let (tree, warnings) = self.export_document_with_analysis()?;
        let collection = sgf::Collection {
            games: vec![tree],
            encoding: "UTF-8".into(),
            warnings: Vec::new(),
        };
        if self.move_trial.is_some() {
            // Export a trial without claiming file ownership or marking the
            // original saved. No-clobber also rejects aliases of existing files.
            storage::write_new(path, sgf::serialize(&collection).into_bytes()).await?;
            self.file_state.save_warnings = warnings;
            self.publish_document()?;
            return Ok(());
        }
        storage::write(path.clone(), sgf::serialize(&collection).into_bytes()).await?;
        self.file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        self.file_state = FileState::new(&self.document, Some(path.clone()));
        self.file_state.save_warnings = warnings;
        self.publish_document()?;
        self.remember_recent_file(&path)
            .await
            .map_err(|source| AppError::RecentFilesAfterSave { path, source })
    }

    pub(super) async fn open_path(&mut self, path: PathBuf) -> Result<(), AppError> {
        let result = async {
            self.ensure_no_move_trial()?;
            let absolute = std::path::absolute(&path)?;
            let bytes = tokio::fs::read(&absolute).await?;
            let document = parse_document(&bytes, super::game_files::is_gib(&absolute))?;
            self.next_revision()?;
            self.remember_recent_file(&absolute).await?;
            self.stop_board_sync().await?;
            self.open_auto_curve_document(document, Some(absolute), true)
                .await
        }
        .await;
        if let Err(error) = &result {
            // Preserve the original open error and still notify the UI even if
            // persistence itself has failed. Never invent a successful open.
            let mut reason = error.to_string();
            if let Ok(absolute) = std::path::absolute(&path) {
                let text = absolute.to_string_lossy();
                let files = self
                    .settings
                    .recent_files
                    .iter()
                    .filter(|saved| saved.as_str() != text.as_ref())
                    .cloned()
                    .collect();
                if let Err(history_error) = self.set_recent_files(files).await {
                    reason.push_str(&format!("; cannot update recent files: {history_error}"));
                }
            }
            self.output.send(AppEvent::OpenFailed {
                path: path.to_string_lossy().into_owned(),
                reason,
            });
        }
        result
    }

    pub(super) async fn open(
        &mut self,
        bytes: Vec<u8>,
        file_path: Option<PathBuf>,
    ) -> Result<(), AppError> {
        self.ensure_no_move_trial()?;
        let document = parse_document(&bytes, false)?;
        self.stop_board_sync().await?;
        self.open_auto_curve_document(document, file_path, true)
            .await
    }

    async fn remember_recent_file(&mut self, path: &std::path::Path) -> Result<(), SettingsError> {
        let text = path.to_string_lossy().into_owned();
        let mut files = self.settings.recent_files.clone();
        files.retain(|saved| saved != &text);
        files.insert(0, text);
        crate::edit::normalize_recent_files(&mut files);
        self.set_recent_files(files).await
    }

    pub(super) async fn set_recent_files(
        &mut self,
        files: Vec<String>,
    ) -> Result<(), SettingsError> {
        if files != self.settings.recent_files {
            let mut settings = self.settings.clone();
            settings.recent_files = files;
            settings.save(&self.directory).await?;
            self.settings = settings;
        }
        Ok(())
    }
}

fn parse_document(bytes: &[u8], gib: bool) -> Result<GameDocument, AppError> {
    let collection = if gib {
        sgf::parse_gib(bytes)?
    } else {
        sgf::parse(bytes).or_else(|error| sgf::parse_gib(bytes).map_err(|_| error))?
    };
    let tree = collection.games.first().ok_or(AppError::EmptyCollection)?;
    Ok(GameDocument::from_sgf(tree)?)
}
