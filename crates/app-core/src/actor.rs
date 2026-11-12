use crate::{
    AnalysisSummary, AnalysisView, AppConfig, AppError, AppEvent, DocumentView, EngineProfile,
    EngineStatus, Settings, VariationPreviewView, VariationTreeView, delivery::Delivery,
};
use analysis::{AnalysisHandle, AnalysisSnapshot, AnalysisStatus, AnalysisSubscription, Focus};
use document::{AnalysisKey, GameDocument, NodeId};
use engine_session::EngineHandle;
use std::{collections::HashMap, future::pending, path::PathBuf};
use tokio::sync::{mpsc, oneshot, watch};

mod analysis_cache;
mod analysis_limit;
mod auto_curve;
mod auto_setup;
mod benchmark;
mod board_sync;
mod clear_analysis;
mod display;
mod editing;
mod fox;
mod game_files;
mod graph_analysis;
mod move_editing;
mod problem_moves;
mod review;
mod signals;
mod strength_estimate;
mod trial;
mod tsumego;
use trial::PendingTrial;

pub(crate) enum Input {
    OpenBytes(Vec<u8>),
    OpenPath(PathBuf),
    OpenDroppedPaths(Vec<PathBuf>),
    Save(PathBuf),
    NewGame(crate::NewGameOptions),
    ConvertToStartPosition,
    StartTrial,
    EndTrial,
    ClearRecentFiles,
    Edit(document::Command),
    Undo,
    Redo,
    SetPondering(bool),
    SetCacheEnabled(bool),
    ClearCache(bool),
    ClearAnalysis(bool),
    SetAutoQuickAnalyzeOnLoad(bool),
    ConfigureEngine(Option<EngineProfile>),
    GenerateTsumego(crate::TsumegoOptions),
    ExitTsumego,
    Shutdown,
}

pub(crate) enum Command {
    SetDisplaySettings(
        crate::DisplaySettings,
        oneshot::Sender<Result<crate::DisplaySettings, AppError>>,
    ),
    AnalysisLimit(crate::analysis_limit::Request),
    Benchmark(
        crate::benchmark::Request,
        oneshot::Sender<Result<crate::BenchmarkStatus, AppError>>,
    ),
    BoardSync(
        crate::board_sync::Request,
        oneshot::Sender<Result<crate::BoardSyncStatus, AppError>>,
    ),
    BeginAutoSetup(oneshot::Sender<Result<crate::auto_setup::Start, AppError>>),
    TryAutoEngine(
        crate::auto_setup::Trial,
        oneshot::Sender<Result<EngineProfile, AppError>>,
    ),
    TsumegoStatus(oneshot::Sender<crate::TsumegoStatus>),
    StrengthEstimate(
        Option<[i32; 2]>,
        oneshot::Sender<Result<crate::StrengthEstimateView, AppError>>,
    ),
    Fox(crate::fox::Request),
    ProblemMoves(
        crate::ProblemMovesQuery,
        oneshot::Sender<Result<crate::ProblemMovesView, AppError>>,
    ),
    SetMoveRankSettings(
        crate::MoveRankSettings,
        oneshot::Sender<Result<crate::MoveRankSettings, AppError>>,
    ),
    Review(
        crate::review::ReviewInput,
        oneshot::Sender<Result<crate::WholeGameProgress, AppError>>,
    ),
    ReviewProgress(oneshot::Sender<Option<crate::WholeGameProgress>>),
    ReviewResult(u32, oneshot::Sender<Result<Option<AnalysisView>, AppError>>),
    ReviewAnalysis(u32, oneshot::Sender<Option<crate::StoredAnalysis>>),
    VariationTree(oneshot::Sender<VariationTreeView>),
    StoneOrigin(
        go_core::Point,
        oneshot::Sender<Result<Option<crate::StoneOriginView>, AppError>>,
    ),
    PreviewVariation(
        u32,
        Vec<go_core::Move>,
        oneshot::Sender<Result<VariationPreviewView, AppError>>,
    ),
    Apply(Input, oneshot::Sender<Result<u64, AppError>>),
    Settings(oneshot::Sender<Settings>),
    TryConfigureEngine(
        crate::EngineSetupProfile,
        PathBuf,
        oneshot::Sender<Result<(), AppError>>,
    ),
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
    file_name: Option<String>,
    file_state: editing::FileState,
    tsumego: Option<tsumego::Session>,
    move_trial: Option<move_editing::TrialSession>,
    tree: VariationTreeView,
    summaries: HashMap<NodeId, AnalysisSummary>,
    graph_analysis: graph_analysis::State,
    review: review::ReviewState,
    strength: Option<strength::Estimator>,
    summary_fingerprint: Option<String>,
    revision: u64,
    directory: PathBuf,
    cache: Option<analysis::persistent_cache::PersistentCache>,
    settings: Settings,
    config: AppConfig,
    running: Option<Running>,
    trial: Option<PendingTrial>,
    auto_setup: auto_setup::State,
    auto_curve: auto_curve::State,
    board_sync: board_sync::State,
    benchmark: benchmark::State,
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
    let cache_path = config
        .cache_directory
        .as_ref()
        .unwrap_or(&directory)
        .join("analysis-cache.sqlite3");
    let cache = analysis::persistent_cache::PersistentCache::spawn(
        cache_path,
        config.cache_max_bytes,
        settings.cache_enabled,
    );
    let mut actor = Actor {
        document: GameDocument::new(),
        file_name: None,
        file_state: editing::FileState::new(&GameDocument::new(), None),
        tsumego: None,
        move_trial: None,
        tree: VariationTreeView::default(),
        summaries: HashMap::new(),
        graph_analysis: Default::default(),
        review: review::ReviewState::default(),
        strength: None,
        summary_fingerprint: None,
        revision: 0,
        directory,
        cache: Some(cache),
        settings,
        config,
        running: None,
        trial: None,
        auto_setup: Default::default(),
        auto_curve: Default::default(),
        board_sync: Default::default(),
        benchmark: Default::default(),
        focus: None,
        output,
    };
    if let Err(error) = actor.publish_document() {
        actor.failed(error.to_string());
    }
    actor.start_engine().await;
    actor.publish_sync();
    actor.publish_analysis_state();
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::SetDisplaySettings(display, reply)) => { let _ = reply.send(actor.set_display_settings(display).await); }
                Some(Command::AnalysisLimit(request)) => {
                    use crate::analysis_limit::Request;
                    match request {
                        Request::Status(reply) => { let _ = reply.send(actor.analysis_state()); }
                        Request::Set(settings, reply) => { let _ = reply.send(actor.set_analysis_limits(settings).await); }
                        Request::Toggle(reply) => {
                            let enabled = !actor.analysis_state().pondering;
                            let result = actor.set_pondering(enabled).await.map(|()| actor.analysis_state());
                            let _ = reply.send(result);
                        }
                    }
                }
                Some(Command::Benchmark(request, reply)) => { let _ = reply.send(actor.benchmark_command(request).await); }
                Some(Command::BoardSync(request, reply)) => { let _ = reply.send(actor.board_sync_command(request).await); }
                Some(Command::TsumegoStatus(reply)) => { let _ = reply.send(actor.tsumego_status()); }
                Some(Command::StrengthEstimate(bounds, reply)) => { let _ = reply.send(actor.strength_estimate(bounds)); }
                Some(Command::Fox(request)) => actor.fox_request(request).await,
                Some(Command::ProblemMoves(query, reply)) => { let _ = reply.send(actor.problem_moves(query)); }
                Some(Command::SetMoveRankSettings(settings, reply)) => { let _ = reply.send(actor.set_move_rank_settings(settings).await); }
                Some(Command::Review(input, reply)) => {
                    actor.benchmark_review_input(&input);
                    let _ = reply.send(actor.review_command(input).await);
                }
                Some(Command::ReviewProgress(reply)) => { let _ = reply.send(actor.review.progress.clone()); }
                Some(Command::ReviewResult(node, reply)) => { let _ = reply.send(actor.review_view(node)); }
                Some(Command::ReviewAnalysis(node, reply)) => { let _ = reply.send(actor.stored_analysis(node)); }
                Some(Command::VariationTree(reply)) => { let _ = reply.send(actor.tree.clone()); }
                Some(Command::StoneOrigin(point, reply)) => { let _ = reply.send(actor.stone_origin(point)); }
                Some(Command::PreviewVariation(node_id, moves, reply)) => {
                    let result = VariationPreviewView::from_document(
                        &actor.document, actor.revision, node_id, &moves,
                    ).map_err(AppError::from);
                    let _ = reply.send(result);
                }
                Some(Command::Settings(reply)) => { let _ = reply.send(actor.settings.clone()); }
                Some(Command::BeginAutoSetup(reply)) => {
                    let result = actor.begin_auto_setup().await;
                    let _ = reply.send(result);
                }
                Some(Command::TryAutoEngine(trial, reply)) => actor.begin_auto_trial(trial, reply).await,
                Some(Command::Apply(Input::Shutdown, reply)) => {
                    actor.auto_setup.cancel();
                    actor.cancel_trial().await;
                    let benchmark_result = actor.stop_benchmark_process().await;
                    let sync_result = actor.stop_board_sync().await;
                    let result = actor.stop_engine().await.and(sync_result).and(benchmark_result).map(|()| actor.revision);
                    if let Some(cache) = actor.cache.take() { cache.shutdown().await; }
                    let _ = reply.send(result);
                    break;
                }
                Some(Command::Apply(input, reply)) => {
                    let _ = reply.send(actor.apply(input).await);
                }
                Some(Command::TryConfigureEngine(profile, source, reply)) => {
                    actor.auto_setup.cancel();
                    actor.begin_trial(profile, source, reply).await;
                }
                None => { actor.cancel_trial().await; let _ = actor.stop_benchmark_process().await; let _ = actor.stop_board_sync().await; let _ = actor.stop_engine().await; break; }
            },
            signal = next_signal(&mut actor.running) => actor.cache_signal(signal),
            result = review::receive(&mut actor.review) => actor.review_signal(result).await,
            result = trial::wait(&mut actor.trial) => actor.finish_trial(result).await,
            result = board_sync::receive(&mut actor.board_sync) => actor.board_sync_line(result).await,
            result = benchmark::receive(&mut actor.benchmark) => actor.benchmark_signal(result).await,
        }
        actor.maybe_start_auto_curve().await;
        actor.publish_analysis_state();
    }
    if let Some(cache) = actor.cache.take() {
        cache.shutdown().await;
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

    fn publish_document(&mut self) -> Result<(), AppError> {
        self.tree.refresh(&self.document, self.revision)?;
        self.refresh_graph_analysis();
        let mut view = DocumentView::from_document(&self.document, self.tree.revision)?;
        view.file_name = self.file_name.clone();
        view.file_path = self
            .file_state
            .path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        view.modified = self.file_state.modified(&self.document);
        view.can_undo = self.file_state.analysis_edits.can_undo(&self.document);
        view.can_redo = self.file_state.analysis_edits.can_redo(&self.document);
        view.warnings.retain(|warning| {
            !self.is_analysis_cleared(NodeId(warning.node))
                || !clear_analysis::is_analysis(&warning.property)
        });
        view.warnings.extend(
            self.file_state
                .save_warnings
                .iter()
                .cloned()
                .map(Into::into),
        );
        view.trial = self.move_trial.is_some();
        view.tree.comment_revision = self.tree.comment_revision;
        for point in &mut view.graph.points {
            let node = NodeId(point.node_id);
            point.summary = self
                .graph_analysis
                .valid_nodes
                .contains(&node)
                .then(|| self.summaries.get(&node).copied())
                .flatten();
        }
        self.output.send(AppEvent::DocumentChanged {
            revision: self.revision,
            view,
        });
        Ok(())
    }

    async fn apply(&mut self, input: Input) -> Result<u64, AppError> {
        match input {
            Input::StartTrial => self.start_move_trial().await?,
            Input::EndTrial => self.end_move_trial().await?,
            Input::GenerateTsumego(options) => self.generate_tsumego(options).await?,
            Input::ExitTsumego => self.exit_tsumego().await?,
            Input::OpenPath(path) => self.open_path(path).await?,
            Input::OpenDroppedPaths(paths) => {
                if let Err(error) = self.ensure_no_move_trial() {
                    if let Some(path) = paths.iter().find(|path| game_files::is_game_file(path)) {
                        self.output.send(AppEvent::OpenFailed {
                            path: path.to_string_lossy().into_owned(),
                            reason: error.to_string(),
                        });
                    }
                    return Err(error);
                }
                for path in paths {
                    if game_files::is_game_file(&path) && self.open_path(path).await.is_ok() {
                        break;
                    }
                }
            }
            Input::OpenBytes(bytes) => self.open(bytes, None).await?,
            Input::Save(path) => self.save_document(path).await?,
            Input::NewGame(options) => self.new_game(options).await?,
            Input::ConvertToStartPosition => self.convert_to_start_position().await?,
            Input::ClearRecentFiles => self.set_recent_files(Vec::new()).await?,
            Input::Edit(command) => {
                self.next_revision()?;
                let before_node = self.document.current();
                let play = matches!(
                    command,
                    document::Command::Play(_) | document::Command::PlayColored(_)
                );
                let navigate = matches!(
                    command,
                    document::Command::Step(_) | document::Command::GoTo(_)
                );
                let before = self.document.revision();
                let before_content = self.document.content_revision();
                self.document.apply(command)?;
                if self.document.content_revision() != before_content {
                    self.file_state.analysis_edits.document_edit();
                }
                if self.document.revision() != before {
                    let resume_limited = !self.sync_active()
                        && (play || (navigate && self.navigation_plays_move(before_node)));
                    self.document_changed_with_resume(resume_limited).await?;
                }
            }
            Input::Undo | Input::Redo => {
                self.edit_history(matches!(input, Input::Undo)).await?;
            }
            Input::ClearAnalysis(all) => self.clear_analysis(all).await?,
            Input::SetCacheEnabled(enabled) => self.set_cache_enabled(enabled).await?,
            Input::ClearCache(all) => self.clear_cache(all).await?,
            Input::SetPondering(enabled) => {
                self.set_pondering(enabled).await?;
            }
            Input::SetAutoQuickAnalyzeOnLoad(enabled) => {
                self.set_auto_quick_analyze_on_load(enabled).await?;
            }
            Input::ConfigureEngine(profile) => {
                self.ensure_no_benchmark()?;
                self.auto_setup.cancel();
                self.cancel_trial().await;
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

    async fn replace_document(&mut self, document: GameDocument) -> Result<(), AppError> {
        self.next_revision()?;
        self.document = document;
        self.file_name = None;
        self.file_state = editing::FileState::new(&self.document, None);
        self.tsumego = None;
        self.document_replaced().await
    }

    /// A new arena can reuse node IDs, so all analysis and tree namespaces
    /// must be reset even when its root happens to have the same board key.
    async fn document_replaced(&mut self) -> Result<(), AppError> {
        self.document_replaced_with_cache(HashMap::new(), false)
            .await
    }

    async fn document_replaced_with_cache(
        &mut self,
        cached: HashMap<AnalysisKey, crate::StoredAnalysis>,
        import_saved: bool,
    ) -> Result<(), AppError> {
        self.flush_cached_analysis().await;
        self.auto_curve.pending = false;
        self.cancel_review().await;
        self.invalidate_review();
        // Opening even an identical game starts a new node-ID namespace.
        // Every live document has a root, so clearing forces the next refresh.
        self.tree.nodes.clear();
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
        // Restore before the first focus: otherwise a fast live result could
        // establish a shallow snapshot before the deep cached payload is seeded.
        self.restore_loaded_analysis(cached)?;
        if import_saved {
            self.import_saved_analysis()?;
        }
        self.document_changed().await
    }

    async fn document_changed(&mut self) -> Result<(), AppError> {
        self.document_changed_with_resume(false).await
    }

    async fn document_changed_with_resume(&mut self, resume_limited: bool) -> Result<(), AppError> {
        self.revision = self.next_revision()?;
        self.output.clear_analysis();
        self.publish_document()?;
        self.update_focus_with_resume(resume_limited).await;
        Ok(())
    }

    async fn update_focus(&mut self) {
        self.update_focus_with_resume(false).await;
    }

    async fn update_focus_with_resume(&mut self, resume_limited: bool) {
        self.publish_saved_focus();
        if self.running.is_none() {
            return;
        }
        let node_id = if self.sync_active() {
            self.board_sync
                .resolved_node
                .filter(|id| self.document.node(*id).is_ok())
                .unwrap_or(self.document.current())
        } else {
            self.document.current()
        };
        let Ok(position) = self.document.position_at(node_id) else {
            self.flush_cached_focus().await;
            self.focus = None;
            if self.limit_reason().is_none()
                && let Some(running) = &self.running
                && let Err(error) = running.coordinator.set_pondering(false).await
            {
                self.analysis_failed(error.to_string());
            }
            return;
        };
        let key = position.analysis_key();
        let changed = self
            .focus
            .as_ref()
            .is_none_or(|focus| focus.node_id != node_id.0 || focus.key != key);
        if changed {
            self.flush_cached_focus().await;
        }
        let restart_review = changed && self.review_running();
        if restart_review && let Err(error) = self.stop_review_query().await {
            self.analysis_failed(error.to_string());
        }
        let view_focus = ViewFocus {
            revision: self.revision,
            node_id: node_id.0,
            key,
            size: position.board.size(),
            to_play: position.board.to_play(),
        };
        let cached = self.cached_focus(node_id, &position).await;
        let focus = Focus {
            revision: self.revision,
            node_id,
            position,
            key,
        };
        if let Some(running) = &self.running {
            // Cached review results establish the same depth floor as LRU hits.
            // Submit live before rebuilding the low-priority batch, reserving a
            // search slot even when background work occupied both threads.
            match running
                .coordinator
                .focus_with_cached_and_resume(focus, cached, resume_limited)
                .await
            {
                Ok(()) => {
                    self.focus = Some(view_focus);
                    if let Err(error) = running.coordinator.set_pondering(self.pondering()).await {
                        self.analysis_failed(error.to_string());
                    }
                }
                Err(error) => self.analysis_failed(error.to_string()),
            }
        }
        if restart_review {
            self.continue_review().await;
        }
    }

    fn coordinator(
        &self,
        engine: &EngineHandle,
        fingerprint: &str,
    ) -> Result<(AnalysisHandle, AnalysisSubscription), AppError> {
        let mut config = self.config.analysis.clone();
        config.engine_fingerprint = fingerprint.to_owned();
        config.pondering = self.pondering();
        config.cache_enabled = self.settings.cache_enabled;
        config.limits = self.settings.analysis_limits.into();
        Ok(analysis::spawn(engine.clone(), config)?)
    }

    async fn start_engine(&mut self) {
        let Some(profile) = &self.settings.engine else {
            self.set_summary_fingerprint(None);
            self.output
                .send(AppEvent::EngineStatusChanged(EngineStatus::Stopped));
            self.publish_saved_focus();
            return;
        };
        let prepared = async {
            let fingerprint = profile.fingerprint().await?;
            let benchmark = self.settings.benchmarks.get(&fingerprint);
            Ok::<_, AppError>((profile.transport_with_benchmark(benchmark)?, fingerprint))
        }
        .await;
        let (transport, fingerprint) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.set_summary_fingerprint(None);
                self.failed(error.to_string());
                self.publish_saved_focus();
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
            let saved = self
                .review
                .results
                .iter()
                .filter(|(_, result)| result.saved.is_some())
                .map(|(&id, result)| (id, result.clone()))
                .collect();
            self.invalidate_review();
            self.review.results = saved;
            if !self.summaries.is_empty() {
                self.summaries
                    .retain(|id, _| self.review.results.contains_key(id));
                // Settings changes do not change the document revision, but
                // clients need a fresh graph before new-engine analysis.
                if let Err(error) = self.publish_document() {
                    self.failed(error.to_string());
                }
            }
        }
    }

    async fn stop_engine(&mut self) -> Result<(), AppError> {
        self.flush_cached_analysis().await;
        self.cancel_review().await;
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
