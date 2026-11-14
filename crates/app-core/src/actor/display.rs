use super::Actor;
use crate::{AppError, DisplaySettings};

impl Actor {
    pub(super) async fn set_display_settings(
        &mut self,
        mut display: DisplaySettings,
    ) -> Result<DisplaySettings, AppError> {
        display.validate().map_err(crate::SettingsError::from)?;
        display.normalize_toolbar_items();
        let mut settings = self.settings.clone();
        settings.display = display.clone();
        settings.save(&self.directory).await?;
        self.settings = settings;
        Ok(display)
    }
}
