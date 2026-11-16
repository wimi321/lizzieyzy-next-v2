use super::Actor;
use crate::{AppError, AppEvent, DisplaySettings};

impl Actor {
    pub(super) async fn set_display_settings(
        &mut self,
        mut display: DisplaySettings,
    ) -> Result<DisplaySettings, AppError> {
        // UI snapshots may predate a native move or another window command.
        // Replace these fields before validation, even when stale bounds are bad.
        crate::WindowLayout::from(&self.settings.display).apply_to(&mut display);
        self.persist_display_settings(display).await
    }

    pub(super) async fn persist_display_settings(
        &mut self,
        mut display: DisplaySettings,
    ) -> Result<DisplaySettings, AppError> {
        display.validate().map_err(crate::SettingsError::from)?;
        display.normalize_toolbar_items();
        let comparison_layout =
            self.double_engine() || display.layout_mode == crate::LayoutMode::DoubleEngine;
        let changed = display != self.settings.display;
        let floating_changed = display.floating_boards != self.settings.display.floating_boards;
        let mut settings = self.settings.clone();
        settings.display = display.clone();
        settings.save(&self.directory).await?;
        self.settings = settings;
        if changed {
            self.output
                .send(AppEvent::DisplaySettingsChanged(display.clone()));
        }
        if floating_changed {
            self.output
                .send(AppEvent::FloatingBoardsChanged(display.floating_boards));
        }
        self.reconcile_comparison().await;
        if comparison_layout {
            self.publish_comparison_changed();
            self.publish_comparison_focus();
        }
        Ok(display)
    }
}
