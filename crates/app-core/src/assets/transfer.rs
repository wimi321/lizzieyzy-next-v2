use std::{path::Path, time::Duration};

use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::watch,
};

use super::{AssetDownloadStatus, AssetError, catalog::AssetSpec};

pub(super) fn client() -> Result<Client, AssetError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(Client::builder()
        .tls_backend_rustls()
        .user_agent("LizzieYzy-Next2/asset-download")
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(30))
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?)
}

pub(crate) async fn verified(path: &Path, spec: &AssetSpec) -> Result<bool, AssetError> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.len() != spec.size {
        return Ok(false);
    }
    let mut file = File::open(path).await?.take(spec.size.saturating_add(1));
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut total = 0;
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        total += read as u64;
    }
    Ok(total == spec.size && format!("{:x}", digest.finalize()).eq_ignore_ascii_case(&spec.sha256))
}

pub(super) async fn transfer(
    spec: &AssetSpec,
    target: &Path,
    file: &mut File,
    events: &watch::Sender<AssetDownloadStatus>,
) -> Result<bool, AssetError> {
    if verified(target, spec).await? {
        return Ok(false);
    }
    // An invalid existing file is never used. Remove only an ordinary managed
    // file; symlinks/directories are errors, never traversed or overwritten.
    match tokio::fs::symlink_metadata(target).await {
        Ok(metadata) if metadata.is_file() => tokio::fs::remove_file(target).await?,
        Ok(_) => return Err(AssetError::Verification),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    // A paused transfer keeps its private temporary file. Rehash that prefix
    // before a Range request, so resumed bytes never bypass the trusted pin.
    file.flush().await?;
    let mut total = file.metadata().await?.len();
    if total > spec.size {
        return Err(AssetError::Verification);
    }
    let mut digest = Sha256::new();
    file.seek(std::io::SeekFrom::Start(0)).await?;
    let mut left = total;
    let mut buffer = [0; 64 * 1024];
    while left > 0 {
        let count = file
            .read(&mut buffer[..left.min(64 * 1024) as usize])
            .await?;
        if count == 0 {
            return Err(AssetError::Verification);
        }
        digest.update(&buffer[..count]);
        left -= count as u64;
    }
    if total == spec.size {
        return if format!("{:x}", digest.finalize()).eq_ignore_ascii_case(&spec.sha256) {
            file.sync_all().await?;
            Ok(true)
        } else {
            Err(AssetError::Verification)
        };
    }
    let mut request = client()?.get(&spec.url);
    if total > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={total}-"));
    }
    let mut response = request.send().await?.error_for_status()?;
    if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let expected = format!("bytes {}-{}/{}", total, spec.size - 1, spec.size);
        if response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|header| header.to_str().ok())
            != Some(expected.as_str())
        {
            return Err(AssetError::Verification);
        }
    } else if response.status() == reqwest::StatusCode::OK {
        // Servers may ignore Range. Restart safely instead of appending a full
        // body to the retained prefix.
        total = 0;
        digest = Sha256::new();
        file.set_len(0).await?;
        file.seek(std::io::SeekFrom::Start(0)).await?;
        events.send_modify(|status| status.downloaded_bytes = 0);
    } else {
        return Err(AssetError::Verification);
    }
    if response
        .content_length()
        .is_some_and(|size| size != spec.size - total)
    {
        return Err(AssetError::Verification);
    }
    while let Some(bytes) = response.chunk().await? {
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or(AssetError::Verification)?;
        if total > spec.size {
            return Err(AssetError::Verification);
        }
        file.write_all(&bytes).await?;
        digest.update(&bytes);
        events.send_modify(|status| status.downloaded_bytes = total);
    }
    if total != spec.size || !format!("{:x}", digest.finalize()).eq_ignore_ascii_case(&spec.sha256)
    {
        return Err(AssetError::Verification);
    }
    file.flush().await?;
    file.sync_all().await?;
    Ok(true)
}
