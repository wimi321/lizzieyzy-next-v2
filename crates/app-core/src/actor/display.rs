use super::Actor;
use crate::{AppError, DisplaySettings};

impl Actor {
    pub(super) async fn set_display_settings(
        &mut self,
        display: DisplaySettings,
    ) -> Result<DisplaySettings, AppError> {
        let mut settings = self.settings.clone();
        settings.display = display.clone();
        settings.save(&self.directory).await?;
        self.settings = settings;
        Ok(display)
    }
}
