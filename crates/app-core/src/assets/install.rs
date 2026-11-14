use super::{
    AssetCatalog, AssetDownloadStatus, AssetError, Command, DownloadPhase, Reply, runtime, transfer,
};
use std::path::{Path, PathBuf};
use tokio::{
    io::AsyncWriteExt,
    sync::{mpsc, oneshot, watch},
};

pub(super) async fn run(
    directory: &Path,
    catalog: &AssetCatalog,
    profile_id: &str,
    commands: &mut mpsc::Receiver<Command>,
    events: &watch::Sender<AssetDownloadStatus>,
) -> bool {
    let (cancel, cancellation) = watch::channel(false);
    let operation = install(directory, catalog, profile_id, &cancellation, events);
    tokio::pin!(operation);
    loop {
        tokio::select! {
            result = &mut operation => { finish(events, result); return false; }
            command = commands.recv() => match command {
                Some(Command::Start(id, reply)) | Some(Command::Install(id, reply)) => {
                    let same = id == profile_id || catalog.get(&id).is_ok_and(|asset| {
                        !asset.weight && profile_for_asset(catalog, &id).as_deref() == Some(profile_id)
                    });
                    let result = if same { Ok(events.borrow().clone()) } else { Err(AssetError::Busy) };
                    let _ = reply.send(result);
                }
                Some(Command::Pause(reply)) => { let _ = reply.send(Err(AssetError::NotApplicable)); }
                Some(Command::ImportHumanSl(_, reply)) => { let _ = reply.send(Err(AssetError::Busy)); }
                Some(Command::Cancel(reply)) => {
                    cancel.send_replace(true);
                    finish(events, operation.await);
                    respond_cancel(events, reply);
                    return false;
                }
                Some(Command::Shutdown(reply)) => {
                    cancel.send_replace(true);
                    finish(events, operation.await);
                    commands.close();
                    respond_shutdown(events, reply);
                    return true;
                }
                None => { cancel.send_replace(true); finish(events, operation.await); return true; }
            }
        }
    }
}

fn respond_cancel(events: &watch::Sender<AssetDownloadStatus>, reply: Reply) {
    let status = events.borrow().clone();
    let result = if status.phase == DownloadPhase::Failed {
        Err(AssetError::Worker(status.error.clone().unwrap_or_default()))
    } else {
        Ok(status)
    };
    let _ = reply.send(result);
}
fn respond_shutdown(
    events: &watch::Sender<AssetDownloadStatus>,
    reply: oneshot::Sender<Result<(), AssetError>>,
) {
    let status = events.borrow().clone();
    let result = if status.phase == DownloadPhase::Failed {
        Err(AssetError::Worker(status.error.clone().unwrap_or_default()))
    } else {
        Ok(())
    };
    let _ = reply.send(result);
}
fn finish(events: &watch::Sender<AssetDownloadStatus>, result: Result<PathBuf, AssetError>) {
    match result {
        Ok(path) => events.send_modify(|status| {
            status.phase = DownloadPhase::Completed;
            status.path = Some(path.to_string_lossy().into_owned());
            status.downloaded_bytes = status.total_bytes;
        }),
        Err(AssetError::Cancelled) => events.send_modify(|status| {
            status.phase = DownloadPhase::Cancelled;
            status.path = None;
        }),
        Err(error) => super::fail(events, error),
    }
}

pub(super) fn profile_for_asset(catalog: &AssetCatalog, id: &str) -> Option<String> {
    catalog.get(id).ok()?.runtime_profile.clone().or_else(|| {
        if id == "runtime-tensorrt" {
            Some("cuda12.8-cudnn9-tensorrt".into())
        } else {
            catalog
                .profiles
                .iter()
                .find(|(_, profile)| profile.assets.iter().any(|asset| asset == id))
                .map(|(id, _)| id.clone())
        }
    })
}

async fn install(
    root: &Path,
    catalog: &AssetCatalog,
    profile_id: &str,
    cancel: &watch::Receiver<bool>,
    events: &watch::Sender<AssetDownloadStatus>,
) -> Result<PathBuf, AssetError> {
    let owned_root = root.to_path_buf();
    let owned_catalog = catalog.clone();
    let id = profile_id.to_owned();
    let initial_cancel = cancel.clone();
    let current = tokio::task::spawn_blocking(move || {
        runtime::inspect_cancellable(&owned_root, &owned_catalog, &id, &initial_cancel)
    })
    .await
    .map_err(|error| AssetError::Worker(error.to_string()))??;
    if current.ready {
        return current.directory.ok_or(AssetError::NotDownloaded);
    }
    let profile = catalog.profile(profile_id)?;
    let mut completed = 0;
    for id in &profile.assets {
        super::runtime_archive::cancelled(cancel)?;
        let spec = catalog.get(id)?;
        let (progress, mut updates) = watch::channel(AssetDownloadStatus::default());
        let operation = fetch(root, spec, cancel, &progress);
        tokio::pin!(operation);
        loop {
            tokio::select! {
                result = &mut operation => { result?; break; }
                result = updates.changed() => {
                    if result.is_err() { break; }
                    let downloaded = updates.borrow().downloaded_bytes;
                    events.send_modify(|status| status.downloaded_bytes = completed + downloaded);
                }
            }
        }
        completed += spec.size;
        events.send_modify(|status| status.downloaded_bytes = completed);
    }
    super::runtime_archive::cancelled(cancel)?;
    events.send_modify(|status| status.phase = DownloadPhase::Installing);
    let owned_root = root.to_path_buf();
    let owned_catalog = catalog.clone();
    let id = profile_id.to_owned();
    let cancellation = cancel.clone();
    let (temporary, target) = tokio::task::spawn_blocking(move || {
        runtime::stage(&owned_root, &owned_catalog, &id, &cancellation)
    })
    .await
    .map_err(|error| AssetError::Worker(error.to_string()))??;
    if *cancel.borrow() {
        tokio::task::spawn_blocking(move || temporary.close())
            .await
            .map_err(|error| AssetError::Worker(error.to_string()))??;
        return Err(AssetError::Cancelled);
    }
    // Publication is the short commit point. Once rename has begun a cancel
    // observes Completed, never a false cancellation of an installed runtime.
    tokio::task::spawn_blocking(move || runtime::publish(temporary, target))
        .await
        .map_err(|error| AssetError::Worker(error.to_string()))?
}

async fn fetch(
    root: &Path,
    spec: &super::catalog::AssetSpec,
    cancel: &watch::Receiver<bool>,
    events: &watch::Sender<AssetDownloadStatus>,
) -> Result<(), AssetError> {
    let target = root.join(spec.relative_path());
    let parent = target
        .parent()
        .ok_or_else(|| AssetError::Archive("missing archive parent".into()))?
        .to_path_buf();
    tokio::fs::create_dir_all(&parent).await?;
    if !tokio::fs::symlink_metadata(&parent).await?.is_dir() {
        return Err(AssetError::Verification);
    }
    let (file, temporary) = tokio::task::spawn_blocking(move || {
        let file = tempfile::Builder::new()
            .prefix(".asset-")
            .suffix(".part")
            .tempfile_in(parent)?;
        Ok::<_, std::io::Error>(file.into_parts())
    })
    .await
    .map_err(|error| AssetError::Worker(error.to_string()))??;
    let mut file = tokio::fs::File::from_std(file);
    let mut cancellation = cancel.clone();
    let result = if *cancel.borrow() {
        Err(AssetError::Cancelled)
    } else {
        tokio::select! {
            result = transfer::transfer(spec, &target, &mut file, events) => result,
            _ = cancellation.changed() => Err(AssetError::Cancelled),
        }
    };
    let _ = file.flush().await;
    drop(file);
    if matches!(result, Ok(true)) {
        tokio::task::spawn_blocking(move || {
            temporary
                .persist_noclobber(target)
                .map_err(|error| AssetError::Io(error.error))
        })
        .await
        .map_err(|error| AssetError::Worker(error.to_string()))??;
        Ok(())
    } else {
        tokio::task::spawn_blocking(move || temporary.close())
            .await
            .map_err(|error| AssetError::Worker(error.to_string()))??;
        result.map(|_| ())
    }
}
