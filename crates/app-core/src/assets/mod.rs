//! Trusted catalog downloads, owned by a single Tokio actor. Callers receive
//! coalesced byte progress; cancellation waits for temporary-file cleanup.

mod acceleration;
mod catalog;
mod install;
mod runtime;
mod runtime_archive;
#[cfg(test)]
mod runtime_test_support;
// Only the Unix runtime tests use it; elsewhere it would be an unused import.
#[cfg(all(test, unix))]
pub(crate) use runtime_test_support::test_runtime_catalog;
mod transfer;
mod types;

use std::path::PathBuf;

use tokio::{
    io::AsyncWriteExt,
    sync::{mpsc, oneshot, watch},
};

pub(crate) use catalog::AssetCatalog;
use catalog::AssetSpec;
pub use types::{
    AccelerationPlan, AccelerationReason, AssetDownloadStatus, AssetError, DownloadPhase,
    OfficialWeight, RuntimeInstallation,
};

type Reply = oneshot::Sender<Result<AssetDownloadStatus, AssetError>>;
enum Command {
    Start(String, Reply),
    Install(String, Reply),
    Cancel(Reply),
    Shutdown(oneshot::Sender<Result<(), AssetError>>),
}

#[derive(Clone, Debug)]
pub struct AssetHandle {
    sender: mpsc::Sender<Command>,
    status: watch::Receiver<AssetDownloadStatus>,
    directory: PathBuf,
    catalog: AssetCatalog,
}

pub fn spawn(
    directory: PathBuf,
) -> Result<(AssetHandle, watch::Receiver<AssetDownloadStatus>), AssetError> {
    Ok(spawn_catalog(directory, AssetCatalog::bundled()?))
}

fn spawn_catalog(
    directory: PathBuf,
    catalog: AssetCatalog,
) -> (AssetHandle, watch::Receiver<AssetDownloadStatus>) {
    let (sender, receiver) = mpsc::channel(16);
    let (events, status) = watch::channel(AssetDownloadStatus::default());
    let handle = AssetHandle {
        sender,
        status: status.clone(),
        directory: directory.clone(),
        catalog: catalog.clone(),
    };
    tokio::spawn(run(directory, catalog, receiver, events));
    (handle, status)
}

#[cfg(test)]
pub(crate) fn spawn_with_catalog(
    directory: PathBuf,
    catalog: AssetCatalog,
) -> (AssetHandle, watch::Receiver<AssetDownloadStatus>) {
    spawn_catalog(directory, catalog)
}

impl AssetHandle {
    fn ensure_open(&self) -> Result<(), AssetError> {
        if self.sender.is_closed() {
            Err(AssetError::Closed)
        } else {
            Ok(())
        }
    }

    pub async fn start(&self, id: String) -> Result<AssetDownloadStatus, AssetError> {
        self.request(|reply| Command::Start(id, reply)).await
    }

    pub async fn install_runtime(
        &self,
        profile_id: String,
    ) -> Result<AssetDownloadStatus, AssetError> {
        self.request(|reply| Command::Install(profile_id, reply))
            .await
    }

    pub fn subscribe_status(&self) -> watch::Receiver<AssetDownloadStatus> {
        self.status.clone()
    }

    pub async fn runtime_installation(
        &self,
        profile_id: String,
    ) -> Result<RuntimeInstallation, AssetError> {
        self.ensure_open()?;
        let root = self.directory.clone();
        let catalog = self.catalog.clone();
        let result =
            tokio::task::spawn_blocking(move || runtime::inspect(&root, &catalog, &profile_id))
                .await
                .map_err(|error| AssetError::Worker(error.to_string()))?;
        self.ensure_open()?;
        result
    }

    pub async fn runtime_for_program(
        &self,
        program: PathBuf,
    ) -> Result<Option<RuntimeInstallation>, AssetError> {
        self.ensure_open()?;
        let root = self.directory.clone();
        let catalog = self.catalog.clone();
        let result =
            tokio::task::spawn_blocking(move || runtime::for_program(&root, &catalog, &program))
                .await
                .map_err(|error| AssetError::Worker(error.to_string()))?;
        self.ensure_open()?;
        result
    }

    pub async fn cancel(&self) -> Result<AssetDownloadStatus, AssetError> {
        self.request(Command::Cancel).await
    }

    pub fn status(&self) -> AssetDownloadStatus {
        self.status.borrow().clone()
    }

    pub async fn list_weights(&self) -> Result<Vec<OfficialWeight>, AssetError> {
        self.ensure_open()?;
        let mut weights = Vec::new();
        for spec in self.catalog.specs.iter().filter(|spec| spec.weight) {
            let path = self.directory.join(spec.relative_path());
            let downloaded = transfer::verified(&path, spec).await?;
            weights.push(spec.weight_view(downloaded, path.to_string_lossy().into_owned()));
        }
        self.ensure_open()?;
        Ok(weights)
    }

    pub async fn verified_weight(&self, id: String) -> Result<PathBuf, AssetError> {
        self.ensure_open()?;
        let spec = self.catalog.get(&id)?;
        if !spec.weight {
            return Err(AssetError::UnknownAsset(id));
        }
        let path = self.directory.join(spec.relative_path());
        if !transfer::verified(&path, spec).await? {
            return Err(AssetError::NotDownloaded);
        }
        self.ensure_open()?;
        Ok(path)
    }

    pub async fn acceleration_plan(&self) -> Result<AccelerationPlan, AssetError> {
        self.ensure_open()?;
        acceleration::detect(&self.catalog).await
    }

    pub async fn shutdown(&self) -> Result<(), AssetError> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Command::Shutdown(reply))
            .await
            .map_err(|_| AssetError::Closed)?;
        receive.await.map_err(|_| AssetError::Closed)?
    }

    async fn request(
        &self,
        command: impl FnOnce(Reply) -> Command,
    ) -> Result<AssetDownloadStatus, AssetError> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(command(reply))
            .await
            .map_err(|_| AssetError::Closed)?;
        receive.await.map_err(|_| AssetError::Closed)?
    }
}

async fn run(
    directory: PathBuf,
    catalog: AssetCatalog,
    mut commands: mpsc::Receiver<Command>,
    events: watch::Sender<AssetDownloadStatus>,
) {
    while let Some(command) = commands.recv().await {
        match command {
            Command::Start(id, reply) => {
                let spec = match catalog.get(&id) {
                    Ok(spec) => spec,
                    Err(error) => {
                        let _ = reply.send(Err(error));
                        continue;
                    }
                };
                if !spec.weight {
                    let applicable = acceleration::detect(&catalog).await.map(|plan| {
                        plan.recommended_assets.contains(&id) || plan.optional_assets.contains(&id)
                    });
                    if !matches!(applicable, Ok(true)) {
                        let _ = reply.send(Err(AssetError::NotApplicable));
                        continue;
                    }
                    let Some(profile_id) = install::profile_for_asset(&catalog, &id) else {
                        let _ = reply.send(Err(AssetError::NotApplicable));
                        continue;
                    };
                    if begin_install(
                        &directory,
                        &catalog,
                        profile_id,
                        reply,
                        &mut commands,
                        &events,
                    )
                    .await
                    {
                        break;
                    }
                    continue;
                }
                let status = AssetDownloadStatus {
                    phase: DownloadPhase::Downloading,
                    asset_id: Some(id),
                    total_bytes: spec.size,
                    ..Default::default()
                };
                events.send_replace(status.clone());
                let _ = reply.send(Ok(status));
                if download(&directory, spec, &mut commands, &events).await {
                    break;
                }
            }
            Command::Install(profile_id, reply) => {
                if begin_install(
                    &directory,
                    &catalog,
                    profile_id,
                    reply,
                    &mut commands,
                    &events,
                )
                .await
                {
                    break;
                }
            }
            Command::Cancel(reply) => {
                let _ = reply.send(Ok(events.borrow().clone()));
            }
            Command::Shutdown(reply) => {
                commands.close();
                let _ = reply.send(Ok(()));
                break;
            }
        }
    }
}

async fn begin_install(
    directory: &std::path::Path,
    catalog: &AssetCatalog,
    profile_id: String,
    reply: Reply,
    commands: &mut mpsc::Receiver<Command>,
    events: &watch::Sender<AssetDownloadStatus>,
) -> bool {
    let total = catalog.profile(&profile_id).and_then(|profile| {
        profile.assets.iter().try_fold(0u64, |total, id| {
            total
                .checked_add(catalog.get(id)?.size)
                .ok_or_else(|| AssetError::Catalog("runtime size overflow".into()))
        })
    });
    let total = match total {
        Ok(total) => total,
        Err(error) => {
            let _ = reply.send(Err(error));
            return false;
        }
    };
    let status = AssetDownloadStatus {
        phase: DownloadPhase::Downloading,
        asset_id: Some(profile_id.clone()),
        total_bytes: total,
        ..Default::default()
    };
    events.send_replace(status.clone());
    let _ = reply.send(Ok(status));
    install::run(directory, catalog, &profile_id, commands, events).await
}

enum Outcome {
    Complete(Result<bool, AssetError>),
    Cancel(Reply),
    Stop(Option<oneshot::Sender<Result<(), AssetError>>>),
}

async fn download(
    directory: &std::path::Path,
    spec: &AssetSpec,
    commands: &mut mpsc::Receiver<Command>,
    events: &watch::Sender<AssetDownloadStatus>,
) -> bool {
    let target = directory.join(spec.relative_path());
    let parent = target
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| directory.to_path_buf());
    let prepared = async {
        tokio::fs::create_dir_all(&parent).await?;
        if !tokio::fs::symlink_metadata(&parent).await?.is_dir() {
            return Err(AssetError::Verification);
        }
        tokio::task::spawn_blocking(move || {
            let file = tempfile::Builder::new()
                .prefix(".asset-")
                .suffix(".part")
                .tempfile_in(parent)?;
            let (file, path) = file.into_parts();
            Ok::<_, std::io::Error>((tokio::fs::File::from_std(file), path))
        })
        .await
        .map_err(|error| AssetError::Worker(error.to_string()))?
        .map_err(AssetError::from)
    }
    .await;
    let (mut file, temporary) = match prepared {
        Ok(pair) => pair,
        Err(error) => {
            fail(events, error);
            return false;
        }
    };
    let outcome = {
        let transfer = transfer::transfer(spec, &target, &mut file, events);
        tokio::pin!(transfer);
        loop {
            tokio::select! {
                result = &mut transfer => break Outcome::Complete(result),
                command = commands.recv() => match command {
                    Some(Command::Install(_, reply)) => { let _ = reply.send(Err(AssetError::Busy)); }
                    Some(Command::Start(id, reply)) => {
                        let result = if id == spec.id { Ok(events.borrow().clone()) } else { Err(AssetError::Busy) };
                        let _ = reply.send(result);
                    }
                    Some(Command::Cancel(reply)) => break Outcome::Cancel(reply),
                    Some(Command::Shutdown(reply)) => break Outcome::Stop(Some(reply)),
                    None => break Outcome::Stop(None),
                }
            }
        }
    };
    // Drain any in-flight Tokio filesystem operation before closing/removing;
    // Windows otherwise can retain the pending write handle after cancellation.
    let _ = file.flush().await;
    drop(file);
    match outcome {
        Outcome::Complete(Ok(true)) => {
            let destination = target.clone();
            let result = tokio::task::spawn_blocking(move || {
                temporary
                    .persist_noclobber(destination)
                    .map_err(|error| AssetError::Io(error.error))
            })
            .await
            .map_err(|error| AssetError::Worker(error.to_string()))
            .flatten();
            match result {
                Ok(()) => complete(events, spec, target),
                Err(error) => fail(events, error),
            }
            false
        }
        other => {
            let cleanup = tokio::task::spawn_blocking(move || temporary.close())
                .await
                .map_err(|error| AssetError::Worker(error.to_string()))
                .and_then(|result| result.map_err(AssetError::from));
            match other {
                Outcome::Complete(result) => {
                    match cleanup.and(result) {
                        Ok(_) => complete(events, spec, target),
                        Err(error) => fail(events, error),
                    }
                    false
                }
                Outcome::Cancel(reply) => {
                    let result = cleanup.map(|()| {
                        events.send_modify(|status| {
                            status.phase = DownloadPhase::Cancelled;
                            status.path = None;
                        });
                        events.borrow().clone()
                    });
                    if let Err(error) = &result {
                        fail(events, error);
                    }
                    let _ = reply.send(result);
                    false
                }
                Outcome::Stop(reply) => {
                    commands.close();
                    if let Err(error) = &cleanup {
                        fail(events, error);
                    } else {
                        events.send_modify(|status| status.phase = DownloadPhase::Cancelled);
                    }
                    if let Some(reply) = reply {
                        let _ = reply.send(cleanup);
                    }
                    true
                }
            }
        }
    }
}

fn complete(events: &watch::Sender<AssetDownloadStatus>, spec: &AssetSpec, target: PathBuf) {
    events.send_modify(|status| {
        status.phase = DownloadPhase::Completed;
        status.downloaded_bytes = spec.size;
        status.path = Some(target.to_string_lossy().into_owned());
    });
}

fn fail(events: &watch::Sender<AssetDownloadStatus>, error: impl std::fmt::Display) {
    events.send_modify(|status| {
        status.phase = DownloadPhase::Failed;
        status.error = Some(error.to_string());
        status.path = None;
    });
}

#[cfg(test)]
mod tests;
