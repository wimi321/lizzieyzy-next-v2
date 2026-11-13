use super::Actor;
use crate::{AppError, UiLanguage};

impl Actor {
    pub(super) async fn set_flash_analysis(
        &mut self,
        flash_analysis: crate::FlashAnalysisSettings,
    ) -> Result<(), AppError> {
        flash_analysis.validate()?;
        let mut settings = self.settings.clone();
        settings.flash_analysis = flash_analysis;
        settings.save(&self.directory).await?;
        self.settings = settings;
        Ok(())
    }

    pub(super) async fn set_always_on_top(&mut self, enabled: bool) -> Result<(), AppError> {
        let mut settings = self.settings.clone();
        settings.always_on_top = enabled;
        settings.save(&self.directory).await?;
        self.settings = settings;
        Ok(())
    }

    pub(super) async fn set_ui_scale(&mut self, scale: f64) -> Result<(), AppError> {
        let mut settings = self.settings.clone();
        settings.ui_scale = scale;
        settings.save(&self.directory).await?;
        self.settings = settings;
        Ok(())
    }

    pub(super) async fn set_play_sound(&mut self, enabled: bool) -> Result<(), AppError> {
        let mut settings = self.settings.clone();
        settings.play_sound = enabled;
        settings.save(&self.directory).await?;
        self.settings = settings;
        Ok(())
    }

    pub(super) async fn set_language(&mut self, language: UiLanguage) -> Result<(), AppError> {
        let mut settings = self.settings.clone();
        settings.language = language;
        settings.save(&self.directory).await?;
        self.settings = settings;
        Ok(())
    }

    pub(super) async fn set_auto_check_updates(&mut self, enabled: bool) -> Result<(), AppError> {
        let mut settings = self.settings.clone();
        settings.auto_check_updates = enabled;
        settings.save(&self.directory).await?;
        self.settings = settings;
        Ok(())
    }

    pub(super) async fn claim_update_check(&mut self, now: u64) -> Result<bool, AppError> {
        const DAY_MS: u64 = 24 * 60 * 60 * 1000;
        if !self.settings.auto_check_updates
            || self
                .settings
                .last_update_check_unix_ms
                .is_some_and(|last| now.saturating_sub(last) < DAY_MS)
        {
            return Ok(false);
        }
        let mut settings = self.settings.clone();
        settings.last_update_check_unix_ms = Some(now);
        settings.save(&self.directory).await?;
        self.settings = settings;
        Ok(true)
    }
}
