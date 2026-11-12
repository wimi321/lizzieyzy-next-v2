use super::{Actor, editing::FileState};
use crate::{AppError, StoneOriginView, TrialError};
use document::GameDocument;
use go_core::Point;

/// Distinct from actor::trial, which owns a candidate engine process.
pub(super) struct TrialSession {
    original: GameDocument,
    original_file_name: Option<String>,
    original_file_state: FileState,
}

impl Actor {
    pub(super) fn stone_origin(&self, point: Point) -> Result<Option<StoneOriginView>, AppError> {
        Ok(self
            .document
            .stone_origin(point)?
            .map(|origin| StoneOriginView {
                revision: self.revision,
                node_id: origin.node.0,
                move_number: origin.move_number,
                review_parent: origin.review_parent.map(|node| node.0),
            }))
    }

    pub(super) fn ensure_no_move_trial(&self) -> Result<(), AppError> {
        if self.move_trial.is_some() {
            return Err(TrialError::DocumentReplacement.into());
        }
        Ok(())
    }

    pub(super) async fn start_move_trial(&mut self) -> Result<(), AppError> {
        if self.move_trial.is_some() {
            return Ok(());
        }
        if self.tsumego.is_some() {
            return Err(TrialError::TsumegoConflict.into());
        }
        // Validate before touching either the document or any live session.
        // Keep ancestral rule history, including ko, and allow navigation back.
        // Only the independent copy loses the continuation and edit history.
        let document = self.document.trial_copy()?;
        self.next_revision()?;
        self.stop_board_sync().await?;
        let original = std::mem::replace(&mut self.document, document);
        let original_file_name = self.file_name.take();
        let original_file_state =
            std::mem::replace(&mut self.file_state, FileState::new(&self.document, None));
        self.file_state
            .inherit_clears(&self.document, &original_file_state);
        self.move_trial = Some(TrialSession {
            original,
            original_file_name,
            original_file_state,
        });
        self.document_replaced().await
    }

    pub(super) async fn end_move_trial(&mut self) -> Result<(), AppError> {
        if self.move_trial.is_none() {
            return Ok(());
        }
        self.next_revision()?;
        if let Some(session) = self.move_trial.take() {
            self.document = session.original;
            self.file_name = session.original_file_name;
            self.file_state = session.original_file_state;
        }
        self.document_replaced().await
    }
}
