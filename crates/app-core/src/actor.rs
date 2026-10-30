use crate::{
    AnalysisSummary, AnalysisView, AppConfig, AppError, AppEvent, DocumentView, EngineProfile,
    EngineStatus, Settings, delivery::Delivery, storage,
};
use analysis::{AnalysisHandle, AnalysisSnapshot, AnalysisStatus, AnalysisSubscription, Focus};
use document::{AnalysisKey, GameDocument, NodeId};
use engine_session::EngineHandle;
use std::{collections::HashMap, future::pending, path::PathBuf};
use tokio::sync::{mpsc, oneshot, watch};

pub(crate) enum Input {
    OpenBytes(Vec<u8>),
    OpenPath(PathBuf),
    Save(PathBuf),
    Edit(document::Command),
    Undo,
    Redo,
    SetPondering(bool),
    ConfigureEngine(Option<EngineProfile>),
    Shutdown,
}

pub(crate) enum Command {
    Apply(Input, oneshot::Sender<Result<u64, AppError>>),
    Settings(oneshot::Sender<Settings>),
}

struct Running {
    engine: EngineHandle,
    coordinator: AnalysisHandle,
    snapshots: AnalysisSubscription,
    engine_status: watch::Receiver<engine_session::EngineStatus>,
    analysis_status: watch::Receiver<AnalysisStatus>,
    fingerprint: String,
}

enum Signal {
    Snapshot(Box<AnalysisSnapshot>),
    Engine(engine_session::EngineStatus),
    Analysis(AnalysisStatus),
}

/// Rendering context belongs to exactly the acknowledged document focus.
/// Retain the size and player so results do not need to replay the document.
struct ViewFocus {
    revision: u64,
    node_id: u32,
    key: AnalysisKey,
    size: go_core::BoardSize,
    to_play: go_core::Color,
}

struct Actor {
    document: GameDocument,
    summaries: HashMap<NodeId, AnalysisSummary>,
    summary_fingerprint: Option<String>,
    revision: u64,
    directory: PathBuf,
    settings: Settings,
    config: AppConfig,
    running: Option<Running>,
    focus: Option<ViewFocus>,
    output: Delivery,
}

pub(crate) async fn run(
    directory: PathBuf,
    settings: Settings,
    config: AppConfig,
    mut commands: mpsc::Receiver<Command>,
    output: Delivery,
) {
    let mut actor = Actor {
        document: GameDocument::new(),
        summaries: HashMap::new(),
        summary_fingerprint: None,
        revision: 0,
        directory,
        settings,
        config,
        running: None,
        focus: None,
        output,
    };
    if let Err(error) = actor.publish_document() {
        actor.failed(error.to_string());
    }
    actor.start_engine().await;
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Settings(reply)) => { let _ = reply.send(actor.settings.clone()); }
                Some(Command::Apply(Input::Shutdown, reply)) => {
                    let result = actor.stop_engine().await.map(|()| actor.revision);
                    let _ = reply.send(result);
                    break;
                }
                Some(Command::Apply(input, reply)) => {
                    let _ = reply.send(actor.apply(input).await);
                }
                None => { let _ = actor.stop_engine().await; break; }
            },
            signal = next_signal(&mut actor.running) => actor.signal(signal),
        }
    }
}

async fn next_signal(running: &mut Option<Running>) -> Signal {
    let Some(running) = running else {
        return pending().await;
    };
    tokio::select! {
        value = running.snapshots.recv() => match value {
            Some(value) => Signal::Snapshot(Box::new(value)),
            None => pending().await,
        },
        changed = running.engine_status.changed() => {
            if changed.is_err() { return pending().await; }
            Signal::Engine(running.engine_status.borrow_and_update().clone())
        },
        changed = running.analysis_status.changed() => {
            if changed.is_err() { return pending().await; }
            Signal::Analysis(running.analysis_status.borrow_and_update().clone())
        },
    }
}

impl Actor {
    fn failed(&self, reason: String) {
        self.output
            .send(AppEvent::EngineStatusChanged(EngineStatus::Failed {
                reason,
                stderr_tail: Vec::new(),
            }));
    }

    fn analysis_failed(&self, reason: String) {
        if let Some(running) = &self.running {
            let status = running.engine_status.borrow().clone();
            if matches!(status, engine_session::EngineStatus::Failed { .. }) {
                self.output
                    .send(AppEvent::EngineStatusChanged(status.into()));
                return;
            }
        }
        self.failed(reason);
    }

    fn signal(&mut self, signal: Signal) {
        match signal {
            Signal::Snapshot(snapshot) => {
                if self.settings.pondering
                    && let Some(focus) = &self.focus
                    && (focus.revision, focus.node_id, focus.key)
                        == (snapshot.revision, snapshot.node_id.0, snapshot.key)
                {
                    match AnalysisView::from_snapshot(*snapshot, focus.size, focus.to_play) {
                        Ok(mut view) => {
                            // Graph history must not gate current candidates:
                            // a coordinator cache miss can publish a shallower
                            // search while the graph retains its best result.
                            let latest = view.summary;
                            view.summary = *self
                                .summaries
                                .entry(NodeId(view.node_id))
                                .and_modify(|best| {
                                    if latest.visits >= best.visits {
                                        *best = latest;
                                    }
                                })
                                .or_insert(latest);
                            self.output.send(AppEvent::AnalysisUpdated(view));
                        }
                        Err(error) => {
                            self.output.clear_analysis();
                            self.analysis_failed(format!("invalid analysis coordinate: {error}"));
                        }
                    }
                }
            }
            Signal::Engine(status) => {
                self.output
                    .send(AppEvent::EngineStatusChanged(status.into()));
            }
            Signal::Analysis(AnalysisStatus::Failed(error)) => {
                // Session failures carry richer diagnostics in EngineStatus.
                // Preserve those instead of replacing stderr with a query error.
                let session_failed = self.running.as_ref().is_some_and(|running| {
                    matches!(
                        *running.engine_status.borrow(),
                        engine_session::EngineStatus::Failed { .. }
                    )
                });
                if !session_failed {
                    self.failed(error.to_string());
                }
            }
            Signal::Analysis(_) => {}
        }
    }

    fn publish_document(&self) -> Result<(), AppError> {
        let mut view = DocumentView::from_document(&self.document)?;
        for point in &mut view.graph.points {
            point.summary = self.summaries.get(&NodeId(point.node_id)).copied();
        }
        self.output.send(AppEvent::DocumentChanged {
            revision: self.revision,
            view,
        });
        Ok(())
    }

    async fn apply(&mut self, input: Input) -> Result<u64, AppError> {
        match input {
            Input::OpenPath(path) => self.open(tokio::fs::read(path).await?).await?,
            Input::OpenBytes(bytes) => self.open(bytes).await?,
            Input::Save(path) => {
                let collection = sgf::Collection {
                    games: vec![self.document.to_sgf()],
                    encoding: "UTF-8".into(),
                    warnings: Vec::new(),
                };
                storage::write(path, sgf::serialize(&collection).into_bytes()).await?;
            }
            Input::Edit(command) => {
                self.next_revision()?;
                let before = self.document.revision();
                self.document.apply(command)?;
                if self.document.revision() != before {
                    self.document_changed().await?;
                }
            }
            Input::Undo | Input::Redo => {
                self.next_revision()?;
                if matches!(input, Input::Undo) {
                    self.document.undo()?;
                } else {
                    self.document.redo()?;
                }
                self.document_changed().await?;
            }
            Input::SetPondering(enabled) => {
                if enabled != self.settings.pondering {
                    let mut settings = self.settings.clone();
                    settings.pondering = enabled;
                    settings.save(&self.directory).await?;
                    self.settings = settings;
                    self.output.clear_analysis();
                    if let Some(running) = &self.running {
                        // Invalid SGF positions remain inspectable but never analyzed.
                        if let Err(error) = running
                            .coordinator
                            .set_pondering(enabled && self.focus.is_some())
                            .await
                        {
                            self.analysis_failed(error.to_string());
                        }
                    }
                }
            }
            Input::ConfigureEngine(profile) => {
                // Validate argument conflicts before changing the saved configuration.
                if let Some(profile) = &profile {
                    profile.transport()?;
                }
                let mut settings = self.settings.clone();
                settings.engine = profile;
                settings.save(&self.directory).await?;
                let changed = settings.engine != self.settings.engine;
                self.settings = settings;
                let healthy = self.running.as_ref().is_some_and(|running| {
                    !matches!(
                        *running.engine_status.borrow(),
                        engine_session::EngineStatus::Failed { .. }
                            | engine_session::EngineStatus::Stopped
                    )
                });
                let unchanged_files = if let (Some(profile), Some(running)) =
                    (&self.settings.engine, &self.running)
                {
                    profile.fingerprint().await.ok().as_ref() == Some(&running.fingerprint)
                } else {
                    false
                };
                // Reapplying a profile also detects replaced model/config files.
                if changed || !healthy || !unchanged_files {
                    self.stop_engine().await?;
                    self.start_engine().await;
                }
            }
            Input::Shutdown => {}
        }
        Ok(self.revision)
    }

    fn next_revision(&self) -> Result<u64, AppError> {
        self.revision.checked_add(1).ok_or(AppError::RevisionLimit)
    }

    async fn open(&mut self, bytes: Vec<u8>) -> Result<(), AppError> {
        let collection = sgf::parse(&bytes)?;
        let tree = collection.games.first().ok_or(AppError::EmptyCollection)?;
        let document = GameDocument::from_sgf(tree)?;
        self.next_revision()?;
        self.document = document;
        self.summaries.clear();
        self.focus = None;
        self.output.clear_analysis();
        // Node IDs are only stable within one document. A fresh coordinator
        // ensures equal root IDs/keys cannot retain another document's query.
        if let Some(mut running) = self.running.take() {
            if let Err(error) = running.coordinator.shutdown().await {
                self.failed(error.to_string());
            }
            match self.coordinator(&running.engine, &running.fingerprint) {
                Ok((coordinator, snapshots)) => {
                    running.analysis_status = coordinator.status();
                    running.coordinator = coordinator;
                    running.snapshots = snapshots;
                    self.running = Some(running);
                }
                Err(error) => {
                    let _ = running.engine.shutdown().await;
                    self.failed(error.to_string());
                }
            }
        }
        self.document_changed().await
    }

    async fn document_changed(&mut self) -> Result<(), AppError> {
        self.revision = self.next_revision()?;
        self.focus = None;
        self.output.clear_analysis();
        self.publish_document()?;
        self.update_focus().await;
        Ok(())
    }

    async fn update_focus(&mut self) {
        let Some(running) = &self.running else {
            return;
        };
        let node_id = self.document.current();
        let Ok(position) = self.document.position_at(node_id) else {
            if let Err(error) = running.coordinator.set_pondering(false).await {
                self.analysis_failed(error.to_string());
            }
            return;
        };
        let key = position.analysis_key();
        let view_focus = ViewFocus {
            revision: self.revision,
            node_id: node_id.0,
            key,
            size: position.board.size(),
            to_play: position.board.to_play(),
        };
        let focus = Focus {
            revision: self.revision,
            node_id,
            position,
            key,
        };
        // In particular, do not special-case comment/revision-only changes:
        // the coordinator retains the query and republishes the current result.
        match running.coordinator.focus(focus).await {
            Ok(()) => {
                self.focus = Some(view_focus);
                if let Err(error) = running
                    .coordinator
                    .set_pondering(self.settings.pondering)
                    .await
                {
                    self.analysis_failed(error.to_string());
                }
            }
            Err(error) => self.analysis_failed(error.to_string()),
        }
    }

    fn coordinator(
        &self,
        engine: &EngineHandle,
        fingerprint: &str,
    ) -> Result<(AnalysisHandle, AnalysisSubscription), AppError> {
        let mut config = self.config.analysis.clone();
        config.engine_fingerprint = fingerprint.to_owned();
        config.pondering = self.settings.pondering;
        Ok(analysis::spawn(engine.clone(), config)?)
    }

    async fn start_engine(&mut self) {
        let Some(profile) = &self.settings.engine else {
            self.set_summary_fingerprint(None);
            self.output
                .send(AppEvent::EngineStatusChanged(EngineStatus::Stopped));
            return;
        };
        let prepared =
            async { Ok::<_, AppError>((profile.transport()?, profile.fingerprint().await?)) }.await;
        let (transport, fingerprint) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.set_summary_fingerprint(None);
                self.failed(error.to_string());
                return;
            }
        };
        self.set_summary_fingerprint(Some(fingerprint.clone()));
        let engine = engine_session::spawn(transport, self.config.session.clone());
        match self.coordinator(&engine, &fingerprint) {
            Ok((coordinator, snapshots)) => {
                let engine_status = engine.status();
                self.output.send(AppEvent::EngineStatusChanged(
                    engine_status.borrow().clone().into(),
                ));
                self.running = Some(Running {
                    engine,
                    analysis_status: coordinator.status(),
                    coordinator,
                    snapshots,
                    engine_status,
                    fingerprint,
                });
                self.update_focus().await;
            }
            Err(error) => {
                let _ = engine.shutdown().await;
                self.failed(error.to_string());
            }
        }
    }

    fn set_summary_fingerprint(&mut self, fingerprint: Option<String>) {
        if self.summary_fingerprint != fingerprint {
            self.summary_fingerprint = fingerprint;
            if !self.summaries.is_empty() {
                self.summaries.clear();
                // Settings changes do not change the document revision, but
                // clients need a fresh graph before new-engine analysis.
                if let Err(error) = self.publish_document() {
                    self.failed(error.to_string());
                }
            }
        }
    }

    async fn stop_engine(&mut self) -> Result<(), AppError> {
        self.focus = None;
        self.output.clear_analysis();
        if let Some(running) = self.running.take() {
            let coordinator_result = running.coordinator.shutdown().await;
            // Always reap the process even if coordinator cleanup failed.
            let engine_result = running.engine.shutdown().await;
            engine_result?;
            coordinator_result?;
        }
        self.output
            .send(AppEvent::EngineStatusChanged(EngineStatus::Stopped));
        Ok(())
    }
}

#[cfg(test)]
mod tests;
