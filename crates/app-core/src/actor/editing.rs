//! File identity, save baselines and recent paths owned by the document actor.

use super::Actor;
use crate::{AppError, AppEvent, NewGameOptions, SettingsError, storage};
use document::GameDocument;
use std::path::PathBuf;

#[derive(Clone)]
pub(super) struct FileState {
    pub path: Option<PathBuf>,
    saved: sgf::GameTree,
}

impl FileState {
    pub fn new(document: &GameDocument, path: Option<PathBuf>) -> Self {
        Self {
            path,
            saved: document.to_sgf(),
        }
    }

    pub fn modified(&self, document: &GameDocument) -> bool {
        // Stable node IDs and cursor position are not file contents. Exact SGF
        // properties/child order also recognize undo back to a saved document.
        self.saved != document.to_sgf()
    }
}

impl Actor {
    pub(super) async fn new_game(&mut self, options: NewGameOptions) -> Result<(), AppError> {
        let document = GameDocument::new_game(options.into())?;
        self.stop_board_sync().await?;
        self.open_auto_curve_document(document, None).await
    }

    pub(super) async fn save_document(&mut self, path: PathBuf) -> Result<(), AppError> {
        let path = std::path::absolute(path)?;
        let collection = sgf::Collection {
            games: vec![self.document.to_sgf()],
            encoding: "UTF-8".into(),
            warnings: Vec::new(),
        };
        storage::write(path.clone(), sgf::serialize(&collection).into_bytes()).await?;
        self.file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        self.file_state = FileState::new(&self.document, Some(path.clone()));
        self.publish_document()?;
        self.remember_recent_file(&path)
            .await
            .map_err(|source| AppError::RecentFilesAfterSave { path, source })
    }

    pub(super) async fn open_path(&mut self, path: PathBuf) -> Result<(), AppError> {
        let result = async {
            let absolute = std::path::absolute(&path)?;
            let bytes = tokio::fs::read(&absolute).await?;
            let document = parse_document(&bytes)?;
            self.next_revision()?;
            self.remember_recent_file(&absolute).await?;
            self.stop_board_sync().await?;
            self.open_auto_curve_document(document, Some(absolute))
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
        let document = parse_document(&bytes)?;
        self.stop_board_sync().await?;
        self.open_auto_curve_document(document, file_path).await
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

fn parse_document(bytes: &[u8]) -> Result<GameDocument, AppError> {
    let collection = sgf::parse(bytes)?;
    let tree = collection.games.first().ok_or(AppError::EmptyCollection)?;
    Ok(GameDocument::from_sgf(tree)?)
}
