use std::{collections::VecDeque, path::PathBuf, time::Duration};

use document::AnalysisKey;
use katago_protocol::AnalysisResponse;
use tokio::{
    sync::{mpsc, watch},
    time::Instant,
};

use super::{CacheError, CacheStatus, Command, Operation, database::Database};

const WRITE_INTERVAL: Duration = Duration::from_secs(1);
pub(super) const MAX_PENDING: usize = 128;

struct Pending {
    key: AnalysisKey,
    engine: String,
    response: Box<AnalysisResponse>,
    due: Instant,
}

struct Actor {
    path: PathBuf,
    max_bytes: u64,
    enabled: bool,
    failed: bool,
    database: Option<Database>,
    pending: VecDeque<Pending>,
    status: watch::Sender<CacheStatus>,
}

pub(super) async fn run(
    path: PathBuf,
    max_bytes: u64,
    enabled: bool,
    mut commands: mpsc::Receiver<Command>,
    mut dropped: watch::Receiver<()>,
    status: watch::Sender<CacheStatus>,
) {
    let mut actor = Actor {
        path,
        max_bytes,
        enabled,
        failed: false,
        database: None,
        pending: VecDeque::new(),
        status,
    };
    let mut watch_drops = true;
    loop {
        let due = actor.pending.front().map(|pending| pending.due);
        tokio::select! {
            notification = dropped.changed(), if watch_drops => {
                if notification.is_ok() {
                    tracing::warn!("analysis cache write queue full; dropped snapshots");
                } else {
                    watch_drops = false;
                }
            }
            () = async {
                match due {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            } => actor.flush_due().await,
            command = commands.recv() => match command {
                Some(Command::Shutdown(reply)) => {
                    actor.stop().await;
                    let _ = reply.send(());
                    return;
                }
                Some(command) => actor.command(command).await,
                None => {
                    actor.stop().await;
                    return;
                }
            }
        }
    }
}

impl Actor {
    async fn command(&mut self, command: Command) {
        match command {
            Command::Put(key, engine, response) => self.enqueue(key, engine, response).await,
            Command::Flush(key, reply) => {
                self.flush(key).await;
                let _ = reply.send(());
            }
            Command::Enable(value, reply) => {
                if !value {
                    self.flush(None).await;
                }
                self.enabled = value;
                if !self.failed {
                    self.status.send_replace(if value {
                        CacheStatus::Enabled
                    } else {
                        CacheStatus::Disabled
                    });
                }
                let _ = reply.send(());
            }
            Command::Invalidate(reason, reply) => {
                self.close().await;
                self.fail(reason);
                let _ = reply.send(());
            }
            Command::Operate(operation, reply) => {
                let response = match operation {
                    Operation::Get(key, engine) => self.get(key, engine).await,
                    Operation::ClearPosition(key) => {
                        self.pending.retain(|pending| pending.key != key);
                        self.operate(Operation::ClearPosition(key)).await
                    }
                    Operation::ClearAll => {
                        self.pending.clear();
                        self.operate(Operation::ClearAll).await
                    }
                    Operation::Put(..) => unreachable!("writes are queued separately"),
                };
                let _ = reply.send(response);
            }
            Command::Shutdown(_) => unreachable!("shutdown is handled by the receive loop"),
            #[cfg(test)]
            Command::Barrier(reply) => {
                let _ = reply.send(self.pending.len());
            }
            #[cfg(test)]
            Command::HoldDatabase {
                operation,
                entered,
                release,
                finished,
            } => {
                let database = self.database.take();
                self.database = tokio::task::spawn_blocking(move || {
                    let _ = entered.send(());
                    let _ = release.blocking_recv();
                    database.map(|mut database| {
                        let _ = database.operate(operation);
                        database
                    })
                })
                .await
                .ok()
                .flatten();
                let _ = finished.send(());
            }
        }
    }

    async fn enqueue(&mut self, key: AnalysisKey, engine: String, response: Box<AnalysisResponse>) {
        if self.failed || !self.enabled {
            return;
        }
        if let Some(pending) = self
            .pending
            .iter_mut()
            .find(|entry| entry.key == key && entry.engine == engine)
        {
            // Keep the latest equally deep final/ownership update while never
            // replacing a deeper result. The window never slides forward.
            if response.root_info.visits >= pending.response.root_info.visits {
                pending.response = response;
            }
            return;
        }
        if self.pending.len() == MAX_PENDING {
            self.flush_oldest().await;
        }
        if !self.failed {
            self.pending.push_back(Pending {
                key,
                engine,
                response,
                due: Instant::now() + WRITE_INTERVAL,
            });
        }
    }

    async fn get(&mut self, key: AnalysisKey, engine: String) -> Option<AnalysisResponse> {
        if self.failed || !self.enabled {
            return None;
        }
        let persisted = self.operate(Operation::Get(key, engine.clone())).await;
        if self.failed {
            return None;
        }
        let pending = self
            .pending
            .iter()
            .find(|entry| entry.key == key && entry.engine == engine);
        match (persisted, pending) {
            (Some(stored), Some(pending))
                if pending.response.root_info.visits >= stored.root_info.visits =>
            {
                Some(*pending.response.clone())
            }
            (None, Some(pending)) => Some(*pending.response.clone()),
            (stored, _) => stored,
        }
    }

    async fn flush(&mut self, key: Option<AnalysisKey>) {
        while let Some(index) = self
            .pending
            .iter()
            .position(|entry| key.is_none_or(|key| entry.key == key))
        {
            if let Some(pending) = self.pending.remove(index) {
                self.write(pending).await;
            }
        }
    }

    async fn flush_due(&mut self) {
        while self
            .pending
            .front()
            .is_some_and(|entry| entry.due <= Instant::now())
        {
            self.flush_oldest().await;
        }
    }

    async fn flush_oldest(&mut self) {
        if let Some(pending) = self.pending.pop_front() {
            self.write(pending).await;
        }
    }

    async fn write(&mut self, pending: Pending) {
        self.operate(Operation::Put(
            pending.key,
            pending.engine,
            pending.response,
        ))
        .await;
    }

    async fn operate(&mut self, operation: Operation) -> Option<AnalysisResponse> {
        if self.failed {
            return None;
        }
        let path = self.path.clone();
        let max_bytes = self.max_bytes;
        let previous = self.database.take();
        let result = tokio::task::spawn_blocking(move || {
            let mut database = match previous {
                Some(database) => database,
                None => Database::open(&path, max_bytes)?,
            };
            let response = database.operate(operation)?;
            Ok::<_, CacheError>((database, response))
        })
        .await;
        match result {
            Ok(Ok((database, response))) => {
                self.database = Some(database);
                response
            }
            result => {
                let reason = match result {
                    Ok(Err(error)) => error.to_string(),
                    Err(error) => error.to_string(),
                    Ok(Ok(_)) => unreachable!(),
                };
                self.fail(reason);
                None
            }
        }
    }

    fn fail(&mut self, reason: String) {
        self.pending.clear();
        if !self.failed {
            tracing::warn!(%reason, "analysis cache disabled; analysis remains available");
            self.failed = true;
            self.status.send_replace(CacheStatus::Failed(reason));
        }
    }

    async fn close(&mut self) {
        let database = self.database.take();
        // Closing SQLite may perform filesystem work too.
        let _ = tokio::task::spawn_blocking(move || drop(database)).await;
    }

    async fn stop(&mut self) {
        self.flush(None).await;
        self.close().await;
        self.status.send_replace(CacheStatus::Stopped);
    }
}
