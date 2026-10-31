//! Resolve the bundled recognizer at the desktop boundary. Program paths are
//! never accepted from IPC; AppCore owns its process and protocol lifecycle.
use std::path::{Path, PathBuf};

use tauri::{Manager, Runtime};

pub struct BoardSyncEnvironment {
    pub program: PathBuf,
}

impl BoardSyncEnvironment {
    pub fn for_app<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<Self, tauri::Error> {
        Ok(Self::from_resource_dir(&app.path().resource_dir()?))
    }

    pub fn from_resource_dir(directory: &Path) -> Self {
        Self {
            program: directory.join("readboard").join("readboard.exe"),
        }
    }
}
