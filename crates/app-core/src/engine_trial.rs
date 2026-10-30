use crate::{EngineProfile, SettingsError};
use serde::{Deserialize, Serialize};
use std::{
    io,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use ts_rs::TS;

/// First-run input. Saved profiles always contain a resolved configuration path.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct EngineSetupProfile {
    pub program: PathBuf,
    pub model: PathBuf,
    pub config: Option<PathBuf>,
    pub extra_args: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineSetupError {
    #[error("engine setup failed: {reason}; stderr: {stderr_tail:?}")]
    Failed {
        reason: String,
        stderr_tail: Vec<String>,
    },
    #[error("engine setup timed out after {0:?} waiting for Ready")]
    TimedOut(Duration),
    #[error("engine setup was cancelled")]
    Cancelled,
    #[error("engine setup timeout must be in (0, 24h]")]
    InvalidTimeout,
    #[error("engine files changed during setup; try again")]
    ChangedFiles,
}

impl EngineSetupProfile {
    pub(crate) async fn resolve(
        self,
        directory: &Path,
        source: PathBuf,
    ) -> Result<EngineProfile, SettingsError> {
        let config = match self.config {
            Some(path) => path,
            None => default_config(directory.to_owned(), source).await?,
        };
        Ok(EngineProfile {
            program: self.program,
            model: self.model,
            config,
            extra_args: self.extra_args,
        })
    }
}

/// Publish only a complete copy, without replacing an existing user-edited file.
async fn default_config(directory: PathBuf, source: PathBuf) -> Result<PathBuf, SettingsError> {
    let path = directory.join("analysis.cfg");
    let target = path.clone();
    tokio::task::spawn_blocking(move || -> io::Result<()> {
        match std::fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_file() => return Ok(()),
            Ok(_) => {
                return Err(io::Error::other(
                    "default configuration is not a regular file",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        std::fs::create_dir_all(directory)?;
        let parent = target
            .parent()
            .ok_or_else(|| io::Error::other("configuration has no parent"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        let mut template = std::fs::File::open(source)?;
        io::copy(&mut template, &mut temporary)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        match temporary.persist_noclobber(&target) {
            Ok(_) => Ok(()),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                if std::fs::symlink_metadata(target)?.is_file() {
                    Ok(())
                } else {
                    Err(io::Error::other(
                        "default configuration is not a regular file",
                    ))
                }
            }
            Err(error) => Err(error.error),
        }
    })
    .await
    .map_err(io::Error::other)
    .and_then(|result| result)
    .map_err(|source| SettingsError::Io {
        kind: "default engine configuration",
        path: path.clone(),
        source,
    })?;
    Ok(path)
}
