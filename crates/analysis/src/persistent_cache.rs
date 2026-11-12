//! Best-effort persistent analysis storage. One task owns the command order;
//! each blocking operation temporarily takes sole ownership of the database.

use std::path::PathBuf;

use document::AnalysisKey;
use katago_protocol::AnalysisResponse;
use tokio::sync::{mpsc, oneshot, watch};

mod actor;
mod database;

pub const DEFAULT_MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CacheStatus {
    Enabled,
    Disabled,
    /// A failure disables caching for this actor's remaining lifetime.
    Failed(String),
    Stopped,
}

#[derive(Debug, thiserror::Error)]
enum CacheError {
    #[error("SQLite cache error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid cached response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("cache filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid cache database: {0}")]
    Invalid(&'static str),
}

enum Operation {
    Get(AnalysisKey, String),
    Put(AnalysisKey, String, Box<AnalysisResponse>),
    ClearPosition(AnalysisKey),
    ClearAll,
}

enum Command {
    Put(AnalysisKey, String, Box<AnalysisResponse>),
    Operate(Operation, oneshot::Sender<Option<AnalysisResponse>>),
    Flush(Option<AnalysisKey>, oneshot::Sender<()>),
    Enable(bool, oneshot::Sender<()>),
    Invalidate(String, oneshot::Sender<()>),
    Shutdown(oneshot::Sender<()>),
    #[cfg(test)]
    Barrier(oneshot::Sender<usize>),
    #[cfg(test)]
    HoldDatabase {
        operation: Operation,
        entered: oneshot::Sender<()>,
        release: oneshot::Receiver<()>,
        finished: oneshot::Sender<()>,
    },
}

/// Writes never wait for the database. Maintenance commands share their FIFO
/// with accepted writes and acknowledge their completion as explicit barriers.
#[derive(Clone, Debug)]
pub struct PersistentCache {
    commands: mpsc::Sender<Command>,
    dropped: watch::Sender<()>,
    status: watch::Receiver<CacheStatus>,
}

impl PersistentCache {
    /// The path is caller-selected within the application's data directory.
    /// Opening is lazy: a disabled cache does not even create its database.
    pub fn spawn(path: PathBuf, max_bytes: u64, enabled: bool) -> Self {
        let (commands, receiver) = mpsc::channel(256);
        let (dropped, dropped_rx) = watch::channel(());
        let (status, status_rx) = watch::channel(if enabled {
            CacheStatus::Enabled
        } else {
            CacheStatus::Disabled
        });
        tokio::spawn(actor::run(
            path, max_bytes, enabled, receiver, dropped_rx, status,
        ));
        Self {
            commands,
            dropped,
            status: status_rx,
        }
    }

    pub fn status(&self) -> watch::Receiver<CacheStatus> {
        self.status.clone()
    }

    pub async fn get(&self, key: AnalysisKey, fingerprint: &str) -> Option<AnalysisResponse> {
        self.operate(Operation::Get(key, fingerprint.to_owned()))
            .await
    }

    /// Returns whether the snapshot was queued. A saturated queue drops this
    /// best-effort write and notifies the actor to log congestion in batches.
    pub fn put(&self, key: AnalysisKey, fingerprint: &str, response: AnalysisResponse) -> bool {
        match self.commands.try_send(Command::Put(
            key,
            fingerprint.to_owned(),
            Box::new(response),
        )) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.dropped.send_replace(());
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }

    /// Persist accepted snapshots before leaving this analysis position.
    pub async fn flush_position(&self, key: AnalysisKey) {
        self.flush_matching(Some(key)).await;
    }

    pub async fn flush(&self) {
        self.flush_matching(None).await;
    }

    async fn flush_matching(&self, key: Option<AnalysisKey>) {
        let (reply, received) = oneshot::channel();
        if self.commands.send(Command::Flush(key, reply)).await.is_ok() {
            let _ = received.await;
        }
    }

    /// Deletes all engine identities for this position. Explicit maintenance
    /// remains available while automatic cache reads and writes are disabled.
    pub async fn clear_position(&self, key: AnalysisKey) {
        self.operate(Operation::ClearPosition(key)).await;
    }

    pub async fn clear_all(&self) {
        self.operate(Operation::ClearAll).await;
    }

    pub async fn set_enabled(&self, enabled: bool) {
        let (reply, received) = oneshot::channel();
        if self
            .commands
            .send(Command::Enable(enabled, reply))
            .await
            .is_ok()
        {
            let _ = received.await;
        }
    }

    /// Reject structurally valid JSON that the caller cannot use as analysis
    /// for its position, without promoting the cache fault to an engine error.
    pub async fn invalidate(&self, reason: &str) {
        let (reply, received) = oneshot::channel();
        if self
            .commands
            .send(Command::Invalidate(reason.to_owned(), reply))
            .await
            .is_ok()
        {
            let _ = received.await;
        }
    }

    pub async fn shutdown(&self) {
        let (reply, received) = oneshot::channel();
        if self.commands.send(Command::Shutdown(reply)).await.is_ok() {
            let _ = received.await;
        }
    }

    async fn operate(&self, operation: Operation) -> Option<AnalysisResponse> {
        let (reply, received) = oneshot::channel();
        self.commands
            .send(Command::Operate(operation, reply))
            .await
            .ok()?;
        received.await.ok().flatten()
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod write_tests;
