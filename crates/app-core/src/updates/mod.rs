//! A single update actor owns release metadata and verified package bytes.
//! The shell supplies the official updater implementation, never an IPC URL.
mod types;

use std::{future::Future, pin::Pin};

use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, oneshot, watch};

pub use types::{UpdateCheck, UpdateDownloadStatus, UpdateError, UpdatePhase, UpdateRelease};

pub type UpdateFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, UpdateError>> + Send + 'a>>;

/// Implementations verify the official updater signature during download and
/// again immediately before installation. AppCore additionally checks SHA-256.
pub trait UpdateBackend: Send + 'static {
    fn check(&mut self) -> UpdateFuture<'_, Option<UpdateRelease>>;
    fn download(
        &mut self,
        progress: watch::Sender<UpdateDownloadStatus>,
    ) -> UpdateFuture<'_, Vec<u8>>;
    fn install(&mut self, bytes: Vec<u8>) -> UpdateFuture<'_, ()>;
}

type StatusReply = oneshot::Sender<Result<UpdateDownloadStatus, UpdateError>>;
enum Command {
    Check(oneshot::Sender<Result<UpdateCheck, UpdateError>>),
    Download(StatusReply),
    Cancel(StatusReply),
    Install(StatusReply),
    Shutdown(oneshot::Sender<Result<(), UpdateError>>),
}

#[derive(Clone, Debug)]
pub struct UpdateHandle {
    sender: mpsc::Sender<Command>,
    status: watch::Receiver<UpdateDownloadStatus>,
}

pub fn spawn(backend: Box<dyn UpdateBackend>) -> UpdateHandle {
    let (sender, commands) = mpsc::channel(16);
    let (events, status) = watch::channel(UpdateDownloadStatus::default());
    tokio::spawn(run(backend, commands, events));
    UpdateHandle { sender, status }
}

impl UpdateHandle {
    pub async fn check(&self) -> Result<UpdateCheck, UpdateError> {
        self.request(Command::Check).await
    }

    /// Returns immediately after starting; completion is delivered as progress.
    pub async fn download(&self) -> Result<UpdateDownloadStatus, UpdateError> {
        self.request(Command::Download).await
    }

    pub async fn cancel(&self) -> Result<UpdateDownloadStatus, UpdateError> {
        self.request(Command::Cancel).await
    }

    pub async fn install(&self) -> Result<UpdateDownloadStatus, UpdateError> {
        self.request(Command::Install).await
    }

    pub fn status(&self) -> UpdateDownloadStatus {
        self.status.borrow().clone()
    }

    pub fn subscribe_status(&self) -> watch::Receiver<UpdateDownloadStatus> {
        self.status.clone()
    }

    pub async fn shutdown(&self) -> Result<(), UpdateError> {
        self.request(Command::Shutdown).await
    }

    async fn request<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<Result<T, UpdateError>>) -> Command,
    ) -> Result<T, UpdateError> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(make(reply))
            .await
            .map_err(|_| UpdateError::Closed)?;
        receive.await.map_err(|_| UpdateError::Closed)?
    }
}

async fn run(
    mut backend: Box<dyn UpdateBackend>,
    mut commands: mpsc::Receiver<Command>,
    events: watch::Sender<UpdateDownloadStatus>,
) {
    let mut release: Option<UpdateRelease> = None;
    let mut downloaded: Option<Vec<u8>> = None;
    while let Some(command) = commands.recv().await {
        match command {
            Command::Check(reply) => {
                // A new check invalidates bytes from the previous release,
                // including when the new manifest is invalid or unavailable.
                downloaded = None;
                release = None;
                events.send_replace(UpdateDownloadStatus::default());
                let result = match check(backend.as_mut(), &mut commands, &events).await {
                    Checked::Complete(result) => result,
                    Checked::Stop(shutdown) => {
                        commands.close();
                        let _ = reply.send(Err(UpdateError::Closed));
                        if let Some(shutdown) = shutdown {
                            let _ = shutdown.send(Ok(()));
                        }
                        return;
                    }
                }
                .map(|checked| {
                    let view = checked
                        .as_ref()
                        .map(|item| item.view.clone())
                        .unwrap_or_default();
                    release = checked;
                    view
                });
                if let Err(error) = &result {
                    fail(&events, error);
                }
                let _ = reply.send(result);
            }
            Command::Download(reply) => {
                let Some(available) = release.as_ref() else {
                    let _ = reply.send(Err(UpdateError::NotAvailable));
                    continue;
                };
                if downloaded.is_some() {
                    let _ = reply.send(Ok(events.borrow().clone()));
                    continue;
                }
                let status = UpdateDownloadStatus {
                    phase: UpdatePhase::Downloading,
                    version: available.view.version.clone(),
                    ..Default::default()
                };
                events.send_replace(status.clone());
                let _ = reply.send(Ok(status));
                let outcome = transfer(backend.as_mut(), &mut commands, &events).await;
                match outcome {
                    Transfer::Complete(result) => {
                        match result.and_then(|bytes| verify_hash(bytes, available.sha256)) {
                            Ok(bytes) => {
                                events.send_modify(|status| {
                                    status.phase = UpdatePhase::Downloaded;
                                    status.downloaded_bytes = bytes.len() as u64;
                                });
                                downloaded = Some(bytes);
                            }
                            Err(error) => fail(&events, &error),
                        }
                    }
                    Transfer::Cancel(reply) => {
                        cancelled(&events);
                        let _ = reply.send(Ok(events.borrow().clone()));
                    }
                    Transfer::Stop(reply) => {
                        commands.close();
                        cancelled(&events);
                        if let Some(reply) = reply {
                            let _ = reply.send(Ok(()));
                        }
                        return;
                    }
                }
            }
            Command::Cancel(reply) => {
                if downloaded.take().is_some() {
                    cancelled(&events);
                }
                let _ = reply.send(Ok(events.borrow().clone()));
            }
            Command::Install(reply) => {
                let Some(bytes) = downloaded.take() else {
                    let _ = reply.send(Err(UpdateError::NotDownloaded));
                    continue;
                };
                events.send_modify(|status| status.phase = UpdatePhase::Installing);
                let result = backend.install(bytes).await.map(|()| {
                    events.send_modify(|status| status.phase = UpdatePhase::Completed);
                    events.borrow().clone()
                });
                if let Err(error) = &result {
                    fail(&events, error);
                }
                let _ = reply.send(result);
            }
            Command::Shutdown(reply) => {
                commands.close();
                let _ = reply.send(Ok(()));
                return;
            }
        }
    }
}

fn verify_hash(bytes: Vec<u8>, expected: [u8; 32]) -> Result<Vec<u8>, UpdateError> {
    let actual: [u8; 32] = Sha256::digest(&bytes).into();
    if actual == expected {
        Ok(bytes)
    } else {
        Err(UpdateError::Verification)
    }
}

enum Transfer {
    Complete(Result<Vec<u8>, UpdateError>),
    Cancel(StatusReply),
    Stop(Option<oneshot::Sender<Result<(), UpdateError>>>),
}

enum Checked {
    Complete(Result<Option<UpdateRelease>, UpdateError>),
    Stop(Option<oneshot::Sender<Result<(), UpdateError>>>),
}

async fn check(
    backend: &mut dyn UpdateBackend,
    commands: &mut mpsc::Receiver<Command>,
    events: &watch::Sender<UpdateDownloadStatus>,
) -> Checked {
    let checking = backend.check();
    tokio::pin!(checking);
    loop {
        tokio::select! {
            result = &mut checking => return Checked::Complete(result),
            command = commands.recv() => match command {
                Some(Command::Check(reply)) => { let _ = reply.send(Err(UpdateError::Busy)); }
                Some(Command::Download(reply) | Command::Install(reply)) => { let _ = reply.send(Err(UpdateError::Busy)); }
                Some(Command::Cancel(reply)) => { let _ = reply.send(Ok(events.borrow().clone())); }
                Some(Command::Shutdown(reply)) => return Checked::Stop(Some(reply)),
                None => return Checked::Stop(None),
            }
        }
    }
}

async fn transfer(
    backend: &mut dyn UpdateBackend,
    commands: &mut mpsc::Receiver<Command>,
    events: &watch::Sender<UpdateDownloadStatus>,
) -> Transfer {
    let downloading = backend.download(events.clone());
    tokio::pin!(downloading);
    loop {
        tokio::select! {
            result = &mut downloading => return Transfer::Complete(result),
            command = commands.recv() => match command {
                Some(Command::Download(reply)) => { let _ = reply.send(Ok(events.borrow().clone())); }
                Some(Command::Check(reply)) => { let _ = reply.send(Err(UpdateError::Busy)); }
                Some(Command::Install(reply)) => { let _ = reply.send(Err(UpdateError::Busy)); }
                Some(Command::Cancel(reply)) => return Transfer::Cancel(reply),
                Some(Command::Shutdown(reply)) => return Transfer::Stop(Some(reply)),
                None => return Transfer::Stop(None),
            }
        }
    }
    // Dropping the download future also drops the official plugin's response
    // stream and in-memory buffer, before the cancellation reply is sent.
}

fn fail(events: &watch::Sender<UpdateDownloadStatus>, error: &UpdateError) {
    events.send_modify(|status| {
        status.phase = UpdatePhase::Failed;
        status.error = Some(error.to_string());
    });
}

fn cancelled(events: &watch::Sender<UpdateDownloadStatus>) {
    events.send_modify(|status| {
        status.phase = UpdatePhase::Cancelled;
        status.error = None;
    });
}

#[cfg(test)]
mod tests;
