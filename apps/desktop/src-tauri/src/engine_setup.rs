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
        let resource_dir = app.path().resource_dir()?;
        let home = app.path().home_dir().ok();
        let install_bases = match std::env::consts::OS {
            "macos" => vec![PathBuf::from("/Applications")],
            "windows" => ["ProgramFiles", "ProgramFiles(x86)"]
                .into_iter()
                .filter_map(std::env::var_os)
                .map(PathBuf::from)
                .collect(),
            // package_release.sh distributes a portable Lizzieyzy/ tree,
            // with no installer or prescribed system installation directory.
            "linux" => vec![PathBuf::from("/opt")],
            _ => Vec::new(),
        };
        let java_install_dirs = java_content_roots(
            std::env::consts::OS,
            &application_dirs,
            home.as_deref(),
            &install_bases,
        );
        let path_dirs = std::env::var_os("PATH")
            .map(|paths| std::env::split_paths(&paths).collect())
            .unwrap_or_default();
        let mut roots = SearchRoots {
            path_dirs,
            application_dirs,
            resource_dirs: vec![resource_dir.clone()],
            java_install_dirs,
            platform_dirs: platform_directory(std::env::consts::OS, std::env::consts::ARCH)
                .map(str::to_owned)
                .into_iter()
                .collect(),
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
            default_config_source: resource_dir.join("katago/analysis.cfg"),
        })
    }
}

fn platform_directory(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("macos-arm64"),
        ("macos", "x86_64") => Some("macos-amd64"),
        ("windows", "x86_64") => Some("windows-x64"),
        ("linux", "x86_64") => Some("linux-x64"),
        _ => None,
    }
}

/// Fixed content roots from Java's package_macos_dmg.sh,
/// package_windows_exe.sh and package_release.sh. No directory enumeration.
fn java_content_roots(
    os: &str,
    application_dirs: &[PathBuf],
    home: Option<&Path>,
    install_bases: &[PathBuf],
) -> Vec<PathBuf> {
    let mut bases = install_bases.to_vec();
    let mut roots = Vec::new();
    match os {
        "macos" => {
            if let Some(home) = home {
                bases.push(home.join("Applications"));
            }
            bases.extend_from_slice(application_dirs);
            roots.extend(
                bases
                    .into_iter()
                    .map(|base| base.join("LizzieYzy Next.app/Contents/app")),
            );
        }
        "windows" => {
            // jpackage's machine-wide installer uses ProgramFiles/<name>/app.
            // Portable archives retain the same name/app layout.
            const NAMES: &[&str] = &[
                "LizzieYzy Next",
                "LizzieYzy Next OpenCL",
                "LizzieYzy Next NVIDIA",
                "LizzieYzy Next NVIDIA TensorRT",
                "LizzieYzy Next DirectML Experimental",
                "LizzieYzy Next OpenVINO Experimental",
                "LizzieYzy Next ROCm gfx103x Experimental",
                "LizzieYzy Next ROCm gfx110x Experimental",
                "LizzieYzy Next ROCm gfx1151 Experimental",
                "LizzieYzy Next ROCm gfx120x Experimental",
            ];
            bases.extend_from_slice(application_dirs);
            for base in bases {
                roots.extend(NAMES.iter().map(|name| base.join(name).join("app")));
            }
            roots.extend(application_dirs.iter().map(|base| base.join("app")));
        }
        "linux" => {
            bases.extend_from_slice(application_dirs);
            if let Some(home) = home {
                bases.push(home.to_owned());
            }
            roots.extend(bases.into_iter().map(|base| base.join("Lizzieyzy")));
        }
        _ => {}
    }
    roots
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
