//! Resolve operating-system locations at the shell boundary. Tests inject a
//! complete environment and never inspect the host's installed engines.

use std::path::{Path, PathBuf};

use app_core::engine_setup::SearchRoots;
use tauri::{Manager, Runtime};

pub struct SetupEnvironment {
    pub roots: SearchRoots,
    pub default_config_source: PathBuf,
}

impl SetupEnvironment {
    pub fn for_app<R: Runtime>(
        app: &tauri::AppHandle<R>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let executable = std::env::current_exe()?;
        #[cfg(target_os = "linux")]
        let appimage = app.env().appimage.map(PathBuf::from);
        #[cfg(not(target_os = "linux"))]
        let appimage: Option<PathBuf> = None;
        let application_dirs = application_directories(&executable, appimage.as_deref());
        let path_dirs = std::env::var_os("PATH")
            .map(|paths| std::env::split_paths(&paths).collect())
            .unwrap_or_default();
        let mut roots = SearchRoots {
            path_dirs,
            application_dirs,
            ..Default::default()
        };
        if cfg!(target_os = "macos") {
            roots.homebrew_prefixes = vec!["/opt/homebrew".into(), "/usr/local".into()];
        }
        if cfg!(windows) {
            roots.windows_dirs.push(PathBuf::from(r"C:\KataGo"));
            for variable in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
                if let Some(directory) = std::env::var_os(variable) {
                    roots
                        .windows_dirs
                        .push(PathBuf::from(directory).join("KataGo"));
                }
            }
            if let Some(home) = std::env::var_os("USERPROFILE") {
                let home = PathBuf::from(home);
                roots.windows_dirs.push(home.join("KataGo"));
                roots
                    .windows_dirs
                    .push(home.join("scoop/apps/katago/current"));
            }
        }
        Ok(Self {
            roots,
            default_config_source: app.path().resource_dir()?.join("katago/analysis.cfg"),
        })
    }
}

fn application_directories(executable: &Path, appimage: Option<&Path>) -> Vec<PathBuf> {
    let mut directories: Vec<_> = executable.parent().map(PathBuf::from).into_iter().collect();
    // Portable engines can live beside the .app or original .AppImage, whose
    // executable actually runs from a bundle or a temporary mounted directory.
    if let Some(bundle) = executable.ancestors().nth(3)
        && bundle
            .extension()
            .is_some_and(|extension| extension == "app")
        && let Some(parent) = bundle.parent()
    {
        directories.push(parent.to_owned());
    }
    if let Some(parent) = appimage.and_then(Path::parent) {
        directories.push(parent.to_owned());
    }
    directories
}

#[cfg(test)]
mod tests;
