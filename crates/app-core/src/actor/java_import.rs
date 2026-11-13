use super::Actor;
use crate::{AppError, JavaImportPreview, Settings, java_import};
use std::path::PathBuf;

impl Actor {
    pub(super) async fn preview_java_import(
        &mut self,
        directory: PathBuf,
    ) -> Result<JavaImportPreview, AppError> {
        self.java_import_sequence = self
            .java_import_sequence
            .checked_add(1)
            .ok_or_else(|| java_import::invalid("preview sequence exhausted"))?;
        let pending = java_import::preview(
            directory,
            &self.directory,
            &self.settings,
            self.java_import_sequence,
        )
        .await?;
        let preview = pending.preview.clone();
        self.pending_java_import = Some(pending);
        Ok(preview)
    }

    pub(super) async fn confirm_java_import(
        &mut self,
        preview_id: String,
    ) -> Result<Settings, AppError> {
        self.ensure_no_benchmark()?;
        let pending = self
            .pending_java_import
            .as_ref()
            .ok_or_else(|| java_import::invalid("no preview is pending"))?;
        if pending.preview.preview_id != preview_id {
            return Err(java_import::invalid("preview token is no longer current"));
        }
        if pending.base != self.settings {
            return Err(java_import::invalid(
                "settings changed since preview; preview the import again",
            ));
        }
        java_import::ensure_separate_directory(&pending.preview.directory, &self.directory).await?;
        let candidate = pending.preview.settings.clone();
        candidate.save(&self.directory).await?;
        self.auto_setup.cancel();
        self.cancel_trial().await;
        let previous = self.settings.engine_settings.clone();
        self.settings = candidate;
        self.pending_java_import = None;
        self.engine_settings_changed(&previous).await?;
        Ok(self.settings.clone())
    }
}
