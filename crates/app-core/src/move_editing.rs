//! Move-editing commands and the read-only origin of a visible stone.

use crate::{AppCore, AppError, actor};
use go_core::{Move, Point};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StoneOriginView {
    #[ts(type = "number")]
    pub revision: u64,
    pub node_id: u32,
    pub move_number: usize,
    /// Java's held-stone review starts immediately before the originating move.
    pub review_parent: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum TrialError {
    #[error("trial play and tsumego cannot be active at the same time")]
    TsumegoConflict,
    #[error("end trial play before replacing the document or starting board sync")]
    DocumentReplacement,
}

impl AppCore {
    pub async fn play_colored(&self, mv: Move) -> Result<u64, AppError> {
        self.apply(document::Command::PlayColored(mv)).await
    }

    pub async fn insert_move(&self, mv: Move) -> Result<u64, AppError> {
        self.apply(document::Command::InsertMove(mv)).await
    }

    pub async fn switch_stone(&self, point: Point) -> Result<u64, AppError> {
        self.apply(document::Command::SwitchStone(point)).await
    }

    pub async fn delete_stone(&self, point: Point) -> Result<u64, AppError> {
        self.apply(document::Command::DeleteStone(point)).await
    }

    pub async fn move_stone(&self, from: Point, to: Point) -> Result<u64, AppError> {
        self.apply(document::Command::MoveStone { from, to }).await
    }

    pub async fn add_variation(&self, moves: Vec<Move>) -> Result<u64, AppError> {
        self.apply(document::Command::AddVariation(moves)).await
    }

    /// Empty points and setup stones have no ordinary move origin.
    /// This does not navigate, edit, publish an event or start analysis.
    pub async fn stone_origin(&self, point: Point) -> Result<Option<StoneOriginView>, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::StoneOrigin(point, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    /// Trial play owns an isolated document copy. Repeated start is a no-op.
    pub async fn start_trial(&self) -> Result<u64, AppError> {
        self.send(actor::Input::StartTrial).await
    }

    /// Restore the original tree, cursor, edit history and saved-file baseline.
    pub async fn end_trial(&self) -> Result<u64, AppError> {
        self.send(actor::Input::EndTrial).await
    }
}
