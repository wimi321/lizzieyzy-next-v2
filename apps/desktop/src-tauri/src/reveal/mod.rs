//! Native file-manager adapter. The command validates the application-data
//! boundary in AppCore before handing a canonical path to this service.
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) trait RevealPlatform: Send + Sync + 'static {
    fn reveal(&self, canonical_path: &Path) -> Result<(), String>;
}

#[derive(Clone)]
pub(crate) struct RevealService {
    platform: Arc<dyn RevealPlatform>,
}

impl Default for RevealService {
    fn default() -> Self {
        Self {
            platform: Arc::new(NativePlatform),
        }
    }
}

impl RevealService {
    pub(crate) async fn reveal(&self, canonical_path: PathBuf) -> Result<(), String> {
        let platform = self.platform.clone();
        tokio::task::spawn_blocking(move || platform.reveal(&canonical_path))
            .await
            .map_err(|error| error.to_string())?
    }

    #[cfg(test)]
    pub(crate) fn with_platform(platform: impl RevealPlatform) -> Self {
        Self {
            platform: Arc::new(platform),
        }
    }
}

struct NativePlatform;

impl RevealPlatform for NativePlatform {
    fn reveal(&self, path: &Path) -> Result<(), String> {
        // Check again if the file disappeared after the core resolved it.
        let metadata = path.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() && !metadata.is_dir() {
            return Err("only files and directories can be revealed".into());
        }
        // Windows uses Shell PIDLs (including extended/UNC path handling);
        // macOS uses NSWorkspace. No shell command text or URL is accepted.
        let revealed =
            tauri_plugin_opener::reveal_item_in_dir(path).map_err(|error| error.to_string());
        #[cfg(target_os = "linux")]
        {
            revealed.or_else(|_| {
                let folder = fallback_folder(path, metadata.is_dir())?;
                let status = linux_fallback_command(folder)
                    .status()
                    .map_err(|error| error.to_string())?;
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("file manager exited with {status}"))
                }
            })
        }
        #[cfg(not(target_os = "linux"))]
        revealed
    }
}

#[cfg(any(test, target_os = "linux"))]
fn fallback_folder(path: &Path, is_directory: bool) -> Result<&Path, String> {
    if is_directory {
        Ok(path)
    } else {
        path.parent()
            .ok_or_else(|| "file has no containing directory".into())
    }
}

#[cfg(any(test, target_os = "linux"))]
fn linux_fallback_command(folder: &Path) -> std::process::Command {
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(folder)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command
}

#[cfg(test)]
mod tests;
