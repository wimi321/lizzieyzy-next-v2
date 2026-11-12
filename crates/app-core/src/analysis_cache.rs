//! Cache controls are serialized with document and engine commands.

use crate::{AppCore, AppError, actor::Input};

impl AppCore {
    pub async fn cache_enabled(&self) -> Result<bool, AppError> {
        Ok(self.settings().await?.cache_enabled)
    }

    pub async fn set_cache_enabled(&self, enabled: bool) -> Result<bool, AppError> {
        self.send(Input::SetCacheEnabled(enabled)).await?;
        Ok(enabled)
    }

    /// Clear all persisted positions, or the current position for every engine.
    /// This does not edit the document or its undo history.
    pub async fn clear_cache(&self, all: bool) -> Result<(), AppError> {
        self.send(Input::ClearCache(all)).await.map(|_| ())
    }

    /// Clear this node's or the whole document's analysis as one undoable edit.
    pub async fn clear_analysis(&self, all: bool) -> Result<u64, AppError> {
        self.send(Input::ClearAnalysis(all)).await
    }
}
