//! Editing inputs and the actor-backed application API.

use crate::{AppCore, AppError, actor};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Explicit new-game values; optional player text is absent from the SGF.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct NewGameOptions {
    pub width: u8,
    pub height: u8,
    pub komi: f64,
    pub handicap: u8,
    pub rules: String,
    pub black_name: Option<String>,
    pub black_rank: Option<String>,
    pub white_name: Option<String>,
    pub white_rank: Option<String>,
}

impl Default for NewGameOptions {
    fn default() -> Self {
        Self {
            width: 19,
            height: 19,
            komi: 7.5,
            handicap: 0,
            rules: "chinese".into(),
            black_name: None,
            black_rank: None,
            white_name: None,
            white_rank: None,
        }
    }
}

impl From<NewGameOptions> for document::NewGameOptions {
    fn from(value: NewGameOptions) -> Self {
        Self {
            width: value.width,
            height: value.height,
            komi: value.komi,
            handicap: value.handicap,
            rules: value.rules,
            black_name: value.black_name,
            black_rank: value.black_rank,
            white_name: value.white_name,
            white_rank: value.white_rank,
        }
    }
}

/// Replaces the editable root metadata together in one undoable command.
/// None removes a text property; Some("") retains an explicit empty SGF value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct GameInfoEdit {
    pub black_name: Option<String>,
    pub black_rank: Option<String>,
    pub white_name: Option<String>,
    pub white_rank: Option<String>,
    pub komi: f64,
    pub rules: String,
    pub result: Option<String>,
    pub date: Option<String>,
    pub event: Option<String>,
}

impl From<GameInfoEdit> for document::GameInfoEdit {
    fn from(value: GameInfoEdit) -> Self {
        Self {
            black_name: value.black_name,
            black_rank: value.black_rank,
            white_name: value.white_name,
            white_rank: value.white_rank,
            komi: value.komi,
            rules: value.rules,
            result: value.result,
            date: value.date,
            event: value.event,
        }
    }
}

impl AppCore {
    pub async fn set_comment(&self, text: String) -> Result<u64, AppError> {
        self.apply(document::Command::SetComment(text)).await
    }

    pub async fn delete_node(&self, node_id: u32) -> Result<u64, AppError> {
        self.apply(document::Command::DeleteNode(document::NodeId(node_id)))
            .await
    }

    pub async fn delete_branch(&self, node_id: u32) -> Result<u64, AppError> {
        self.apply(document::Command::DeleteBranch(document::NodeId(node_id)))
            .await
    }

    pub async fn promote_to_main(&self, node_id: u32) -> Result<u64, AppError> {
        self.apply(document::Command::PromoteToMain(document::NodeId(node_id)))
            .await
    }

    pub async fn set_game_info(&self, info: GameInfoEdit) -> Result<u64, AppError> {
        self.apply(document::Command::SetGameInfo(info.into()))
            .await
    }

    pub async fn new_game(&self, options: NewGameOptions) -> Result<u64, AppError> {
        self.send(actor::Input::NewGame(options)).await
    }

    /// Newest first, at most five absolute file paths, matching Java's menu.
    pub async fn recent_files(&self) -> Result<Vec<String>, AppError> {
        Ok(self.settings().await?.recent_files)
    }

    pub async fn clear_recent_files(&self) -> Result<(), AppError> {
        self.send(actor::Input::ClearRecentFiles).await.map(|_| ())
    }
}

pub(crate) fn normalize_recent_files(files: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    files.retain(|path| !path.is_empty() && seen.insert(path.clone()));
    files.truncate(5);
}
