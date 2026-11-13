//! The main window applies persisted preferences; tests substitute the platform.
use app_core::AppCore;
use tauri::{Manager, Runtime};

use crate::error::CommandError;

pub(crate) trait WindowPlatform: Send + Sync + 'static {
    fn set_always_on_top(&self, enabled: bool) -> Result<(), String>;
}

pub(crate) struct WindowPreferences {
    platform: Box<dyn WindowPlatform>,
    changes: tokio::sync::Mutex<()>,
}

impl WindowPreferences {
    pub(crate) fn for_app<R: Runtime>(app: tauri::AppHandle<R>) -> Self {
        Self {
            platform: Box::new(MainWindow(app)),
            changes: tokio::sync::Mutex::new(()),
        }
    }

    pub(crate) async fn initialize(&self, core: &AppCore) -> Result<(), CommandError> {
        self.apply(core.always_on_top().await?)
    }

    pub(crate) fn apply(&self, enabled: bool) -> Result<(), CommandError> {
        self.platform
            .set_always_on_top(enabled)
            .map_err(CommandError::WindowUnavailable)
    }

    pub(crate) async fn set(&self, core: &AppCore, enabled: bool) -> Result<bool, CommandError> {
        // Serialize the persistence and native effect together: concurrent IPC
        // calls cannot apply an older value after a newer value has been saved.
        let _change = self.changes.lock().await;
        let previous = core.always_on_top().await?;
        let saved = core.set_always_on_top(enabled).await?;
        if let Err(native_error) = self.apply(saved) {
            if let Err(rollback_error) = core.set_always_on_top(previous).await {
                return Err(CommandError::WindowUnavailable(format!(
                    "{native_error}; could not restore the previous saved preference: {rollback_error}; the requested preference may remain saved"
                )));
            }
            return Err(native_error);
        }
        Ok(saved)
    }

    #[cfg(test)]
    pub(crate) fn with_platform(platform: impl WindowPlatform) -> Self {
        Self {
            platform: Box::new(platform),
            changes: tokio::sync::Mutex::new(()),
        }
    }
}

struct MainWindow<R: Runtime>(tauri::AppHandle<R>);

impl<R: Runtime> WindowPlatform for MainWindow<R> {
    fn set_always_on_top(&self, enabled: bool) -> Result<(), String> {
        let window = self
            .0
            .get_webview_window("main")
            .ok_or_else(|| "main window is unavailable".to_owned())?;
        window
            .set_always_on_top(enabled)
            .map_err(|error| error.to_string())
    }
}
