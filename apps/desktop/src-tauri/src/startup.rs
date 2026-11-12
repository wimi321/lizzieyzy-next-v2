//! Startup argument handling and the boundary between the shell and AppCore.

use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use app_core::{AppCore, AppError, AppSubscription};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct StartupOptions {
    pub config_dir: Option<PathBuf>,
    pub document: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error("--config-dir requires a nonempty directory path")]
    MissingConfigDirectory,
    #[error("--config-dir may only be specified once")]
    DuplicateConfigDirectory,
    #[error("unknown startup option: {0:?}")]
    UnknownOption(OsString),
    #[error("only one startup document may be specified")]
    MultipleDocuments,
    #[error("the startup document path must not be empty")]
    EmptyDocument,
    #[error("cannot create configuration directory {path}: {source}")]
    ConfigDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Core(Box<AppError>),
}

impl From<AppError> for StartupError {
    fn from(error: AppError) -> Self {
        Self::Core(Box::new(error))
    }
}

impl StartupOptions {
    /// Parse arguments after the executable name. Paths stay as OsString so
    /// native paths never need a lossy UTF-8 conversion.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, StartupError> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        let mut positional_only = false;
        while let Some(arg) = args.next() {
            if !positional_only && arg == "--" {
                positional_only = true;
                continue;
            }
            let directory = if positional_only {
                None
            } else {
                inline_value(&arg, "--config-dir=")
            };
            if !positional_only && (arg == "--config-dir" || directory.is_some()) {
                if options.config_dir.is_some() {
                    return Err(StartupError::DuplicateConfigDirectory);
                }
                let path = match directory {
                    Some(path) => path,
                    None => {
                        let path = args.next().ok_or(StartupError::MissingConfigDirectory)?;
                        if path.to_string_lossy().starts_with('-') {
                            return Err(StartupError::MissingConfigDirectory);
                        }
                        path
                    }
                };
                if path.is_empty() {
                    return Err(StartupError::MissingConfigDirectory);
                }
                options.config_dir = Some(path.into());
                continue;
            }
            let document = if positional_only {
                arg
            } else if let Some(path) = inline_value(&arg, "--open=") {
                path
            } else if arg.to_string_lossy().starts_with('-') {
                return Err(StartupError::UnknownOption(arg));
            } else {
                arg
            };
            if document.is_empty() {
                return Err(StartupError::EmptyDocument);
            } else if options.document.replace(document.into()).is_some() {
                return Err(StartupError::MultipleDocuments);
            }
        }
        Ok(options)
    }
}

// Strip only the ASCII option prefix, preserving every native path code unit.
// OsStr slicing is not stable on the workspace toolchain, so use safe OS APIs.
#[cfg(unix)]
fn inline_value(arg: &OsStr, prefix: &str) -> Option<OsString> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    arg.as_bytes()
        .strip_prefix(prefix.as_bytes())
        .map(|value| OsString::from_vec(value.to_vec()))
}

#[cfg(windows)]
fn inline_value(arg: &OsStr, prefix: &str) -> Option<OsString> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let mut units = arg.encode_wide();
    for expected in prefix.encode_utf16() {
        if units.next() != Some(expected) {
            return None;
        }
    }
    Some(OsString::from_wide(&units.collect::<Vec<_>>()))
}

/// Initialize AppCore before registering the shell state. File failures are
/// delivered as OpenFailed, keeping the application usable like a failed drop.
pub async fn start(
    options: StartupOptions,
    default_dir: PathBuf,
) -> Result<(AppCore, AppSubscription), StartupError> {
    start_with_data_directory(options, default_dir.clone(), default_dir).await
}

/// The normal shell keeps the disposable database in the application data
/// directory. A portable --config-dir isolates both settings and cache.
pub async fn start_with_data_directory(
    options: StartupOptions,
    default_dir: PathBuf,
    data_dir: PathBuf,
) -> Result<(AppCore, AppSubscription), StartupError> {
    let cache_directory = options.config_dir.clone().unwrap_or(data_dir);
    let directory = options.config_dir.unwrap_or(default_dir);
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|source| StartupError::ConfigDirectory {
            path: directory.clone(),
            source,
        })?;
    let config = app_core::AppConfig {
        cache_directory: Some(cache_directory),
        ..Default::default()
    };
    let (core, subscription) = app_core::spawn_with_config(directory, config).await?;
    if let Some(document) = options.document
        && let Err(error) = crate::system_open::open_paths(&core, vec![document]).await
    {
        eprintln!("startup file open: {error}");
    }
    Ok((core, subscription))
}

#[cfg(test)]
mod tests;
