use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::oneshot,
};

use super::{
    AssetCatalog, AssetError, AssetHandle, Command, DownloadPhase, HumanSlModelStatus, transfer,
};

pub(crate) const HUMAN_SL_ID: &str = "human-sl";

impl AssetHandle {
    pub async fn humansl_status(&self) -> Result<HumanSlModelStatus, AssetError> {
        self.ensure_open()?;
        let status = self.status();
        if status.asset_id.as_deref() == Some(HUMAN_SL_ID)
            && matches!(
                status.phase,
                DownloadPhase::Downloading | DownloadPhase::Paused
            )
        {
            return Ok(HumanSlModelStatus::Downloading {
                downloaded_bytes: status.downloaded_bytes,
                total_bytes: status.total_bytes,
                paused: status.phase == DownloadPhase::Paused,
            });
        }
        let spec = self.catalog.get(HUMAN_SL_ID)?;
        let path = self.directory.join(spec.relative_path());
        match tokio::fs::symlink_metadata(&path).await {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(HumanSlModelStatus::Missing)
            }
            Err(error) => Err(error.into()),
            Ok(_) if transfer::verified(&path, spec).await? => {
                Ok(HumanSlModelStatus::Ready { path })
            }
            Ok(_) => Ok(HumanSlModelStatus::Corrupt),
        }
    }

    /// Explicit external profiles and managed downloads use the same trusted pin.
    pub async fn verified_humansl_at(&self, path: Option<PathBuf>) -> Result<PathBuf, AssetError> {
        self.ensure_open()?;
        let spec = self.catalog.get(HUMAN_SL_ID)?;
        let path = path.unwrap_or_else(|| self.directory.join(spec.relative_path()));
        if !transfer::verified(&path, spec).await? {
            return Err(AssetError::NotDownloaded);
        }
        self.ensure_open()?;
        Ok(path)
    }

    pub async fn import_humansl(&self, source: PathBuf) -> Result<HumanSlModelStatus, AssetError> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Command::ImportHumanSl(source, reply))
            .await
            .map_err(|_| AssetError::Closed)?;
        receive.await.map_err(|_| AssetError::Closed)?
    }
}

pub(super) async fn import(
    directory: &Path,
    catalog: &AssetCatalog,
    source: PathBuf,
) -> Result<HumanSlModelStatus, AssetError> {
    let spec = catalog.get(HUMAN_SL_ID)?;
    if !transfer::verified(&source, spec).await? {
        return Err(AssetError::Verification);
    }
    let target = directory.join(spec.relative_path());
    if transfer::verified(&target, spec).await? {
        return Ok(HumanSlModelStatus::Ready { path: target });
    }
    let parent = directory.join("weights");
    tokio::fs::create_dir_all(&parent).await?;
    if !tokio::fs::symlink_metadata(&parent).await?.is_dir() {
        return Err(AssetError::Verification);
    }
    let (output, temporary) = tokio::task::spawn_blocking(move || {
        let output = tempfile::Builder::new()
            .prefix(".human-sl-")
            .suffix(".part")
            .tempfile_in(parent)?;
        Ok::<_, std::io::Error>(output.into_parts())
    })
    .await
    .map_err(|error| AssetError::Worker(error.to_string()))??;
    let mut output = tokio::fs::File::from_std(output);
    let mut input = tokio::fs::File::open(&source)
        .await?
        .take(spec.size.saturating_add(1));
    tokio::io::copy(&mut input, &mut output).await?;
    output.flush().await?;
    output.sync_all().await?;
    drop(output);
    // Recheck the actual copy in case the source changed while copying.
    if !transfer::verified(&temporary, spec).await? {
        return Err(AssetError::Verification);
    }
    match tokio::fs::symlink_metadata(&target).await {
        Ok(metadata) if metadata.is_file() => tokio::fs::remove_file(&target).await?,
        Ok(_) => return Err(AssetError::Verification),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let destination = target.clone();
    tokio::task::spawn_blocking(move || {
        temporary
            .persist_noclobber(destination)
            .map_err(|error| error.error)
    })
    .await
    .map_err(|error| AssetError::Worker(error.to_string()))??;
    Ok(HumanSlModelStatus::Ready { path: target })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HumanModelBackend {
    Supported,
    TensorRt,
    Unknown,
}

/// Pins establish the backend even when the executable was renamed. Otherwise
/// explicit backend names are hints; unknown executables can still try HumanSL
/// and recover their original profile if loading fails.
pub(crate) async fn human_model_backend(profile: &crate::EngineProfile) -> HumanModelBackend {
    if let Ok(file) = tokio::fs::File::open(&profile.program).await
        && let Ok(metadata) = file.metadata().await
        && metadata.is_file()
        && metadata.len() <= 256 * 1024 * 1024
    {
        let mut file = file.take(metadata.len().saturating_add(1));
        let mut digest = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        let mut valid = true;
        let mut total = 0;
        loop {
            match file.read(&mut buffer).await {
                Ok(0) => break,
                Ok(count) => {
                    total += count as u64;
                    digest.update(&buffer[..count]);
                }
                Err(_) => {
                    valid = false;
                    break;
                }
            }
        }
        if valid
            && total == metadata.len()
            && let Some(backend) =
                super::catalog::backend_for_digest(&format!("{:x}", digest.finalize()))
        {
            return if backend.eq_ignore_ascii_case("tensorrt") {
                HumanModelBackend::TensorRt
            } else {
                HumanModelBackend::Supported
            };
        }
    }
    backend_from_name(&profile.program.to_string_lossy())
}

fn backend_from_name(name: &str) -> HumanModelBackend {
    let name = name.to_ascii_lowercase();
    let executable = name.rsplit(['/', '\\']).next().unwrap_or(&name);
    let tokens = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .collect::<Vec<_>>();
    if executable.contains("tensorrt") || executable.contains("trt") {
        HumanModelBackend::TensorRt
    } else if tokens.iter().any(|name| {
        matches!(
            *name,
            "eigen" | "cpu" | "cuda" | "opencl" | "metal" | "directml" | "openvino" | "rocm"
        )
    }) {
        HumanModelBackend::Supported
    } else {
        HumanModelBackend::Unknown
    }
}

#[cfg(test)]
mod tests;
