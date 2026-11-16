use super::{AssetDownloadStatus, AssetError, DownloadPhase, transfer};
use crate::assets::catalog::AssetSpec;
use std::path::Path;
use tokio::{fs::File, io::AsyncWriteExt, sync::watch};

pub(super) struct Pause<'a> {
    requested: watch::Receiver<bool>,
    acknowledged: watch::Sender<()>,
    events: &'a watch::Sender<AssetDownloadStatus>,
}

impl<'a> Pause<'a> {
    pub(super) fn new(
        events: &'a watch::Sender<AssetDownloadStatus>,
    ) -> (watch::Sender<bool>, watch::Receiver<()>, Self) {
        let (request, requested) = watch::channel(false);
        let (acknowledged, acknowledgement) = watch::channel(());
        (
            request,
            acknowledgement,
            Self {
                requested,
                acknowledged,
                events,
            },
        )
    }

    pub(super) async fn wait(
        &self,
        cancel: &watch::Receiver<bool>,
        downloaded: Option<u64>,
    ) -> Result<(), AssetError> {
        let mut requested = self.requested.clone();
        if !*requested.borrow_and_update() {
            return Ok(());
        }
        let mut cancellation = cancel.clone();
        loop {
            self.events.send_modify(|status| {
                status.phase = DownloadPhase::Paused;
                if let Some(downloaded) = downloaded {
                    status.downloaded_bytes = downloaded;
                }
            });
            self.acknowledged.send_replace(());
            tokio::select! {
                changed = requested.changed() => {
                    if changed.is_err() { return Err(AssetError::Cancelled); }
                    if !*requested.borrow_and_update() { return Ok(()); }
                    // Resume followed immediately by pause can coalesce before
                    // this future runs. It is still safely paused: acknowledge
                    // the new request instead of waiting for another resume.
                }
                _ = cancellation.wait_for(|cancelled| *cancelled) => return Err(AssetError::Cancelled),
            }
        }
    }

    pub(super) async fn transfer(
        &self,
        spec: &AssetSpec,
        target: &Path,
        file: &mut File,
        cancel: &watch::Receiver<bool>,
        events: &watch::Sender<AssetDownloadStatus>,
        completed: u64,
    ) -> Result<bool, AssetError> {
        let mut requested = self.requested.clone();
        let mut cancellation = cancel.clone();
        loop {
            super::super::runtime_archive::cancelled(cancel)?;
            let result = tokio::select! {
                result = transfer::transfer(spec, target, file, events) => Some(result),
                _ = cancellation.wait_for(|cancelled| *cancelled) => return Err(AssetError::Cancelled),
                _ = requested.wait_for(|paused| *paused) => None,
            };
            if let Some(result) = result {
                return result;
            }
            // Dropping transfer closes its HTTP response. Drain pending writes
            // before acknowledging pause, particularly for Windows file handles.
            file.flush().await?;
            let received = file.metadata().await?.len();
            events.send_modify(|status| status.downloaded_bytes = received);
            self.wait(cancel, Some(completed + received)).await?;
            // transfer rehashes this retained prefix and validates HTTP Range.
        }
    }
}
