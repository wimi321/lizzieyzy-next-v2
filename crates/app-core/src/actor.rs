use crate::{
    AnalysisSummary, AnalysisView, AppConfig, AppError, AppEvent, EngineProfile, EngineStatus,
    Settings, VariationPreviewView, VariationTreeView, delivery::Delivery,
};
use analysis::{AnalysisHandle, AnalysisSnapshot, AnalysisStatus, AnalysisSubscription, Focus};
use document::{AnalysisKey, GameDocument, NodeId};
use engine_session::EngineHandle;
use std::{collections::HashMap, future::pending, path::PathBuf};
use tokio::sync::{mpsc, oneshot, watch};

mod acceleration;
mod analysis_cache;
mod analysis_extras;
mod analysis_limit;
mod analysis_tools;
mod auto_curve;
mod auto_setup;
mod batch;
mod benchmark;
mod board_sync;
mod capture;
mod clear_analysis;
mod coach;
mod commentary;
mod comparison;
mod coordinators;
mod display;
mod document_view;
mod editing;
mod engine_settings;
mod engines;
mod everyday;
mod focus;
mod fox;
mod game;
mod game_files;
mod graph_analysis;
mod gtp;
mod java_import;
mod match_game;
mod move_editing;
mod preferences;
mod problem_moves;
mod release;
mod remote;
mod review;
mod saves;
mod scoring;
mod signals;
mod strength_estimate;
mod tencent;
mod trial;
mod tsumego;
mod web_board;
mod weights;
mod windows;
mod zhizi;
use trial::PendingTrial;

mod messages;
pub(crate) use messages::{Command, Input};

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
    capture: capture::State,
    comparison: comparison::State,
    web_board: web_board::State,
    restriction: Option<crate::AnalysisRestriction>,
    restricted_analysis: Option<crate::AnalysisView>,
    scoring: Option<scoring::Session>,
    document: GameDocument,
    saves: saves::State,
    file_name: Option<String>,
    file_state: editing::FileState,
    tsumego: Option<tsumego::Session>,
    move_trial: Option<move_editing::TrialSession>,
    tree: VariationTreeView,
    summaries: HashMap<NodeId, AnalysisSummary>,
    graph_analysis: graph_analysis::State,
    review: review::ReviewState,
    batch: batch::State,
    game: Option<game::Session>,
    coach: coach::State,
    commentary: commentary::State,
    remote: crate::remote::RemoteSecrets,
    zhizi: crate::zhizi::ZhiziService,
    startup_local_fallback: bool,
    gtp: gtp::State,
    match_session: Option<match_game::Session>,
    strength: Option<strength::Estimator>,
    summary_fingerprint: Option<String>,
    revision: u64,
    directory: PathBuf,
    cache: Option<analysis::persistent_cache::PersistentCache>,
    settings: Settings,
    pending_java_import: Option<crate::java_import::PendingJavaImport>,
    java_import_sequence: u64,
    config: AppConfig,
    running: Option<Running>,
    trial: Option<PendingTrial>,
    auto_setup: auto_setup::State,
    auto_curve: auto_curve::State,
    board_sync: board_sync::State,
    benchmark: benchmark::State,
    assets: weights::State,
    acceleration: acceleration::State,
    support: Option<crate::support::SupportHandle>,
    updates: release::State,
    focus: Option<ViewFocus>,
    output: Delivery,
}

pub(crate) async fn run(
    directory: PathBuf,
    settings: Settings,
    config: AppConfig,
    mut commands: mpsc::Receiver<Command>,
    output: Delivery,
    support: crate::support::SupportHandle,
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
        capture: Default::default(),
        comparison: Default::default(),
        web_board: Default::default(),
        restriction: None,
        restricted_analysis: None,
        scoring: None,
        document: GameDocument::new(),
        saves: Default::default(),
        file_name: None,
        file_state: editing::FileState::new(&GameDocument::new(), None),
        tsumego: None,
        move_trial: None,
        tree: VariationTreeView::default(),
        summaries: HashMap::new(),
        graph_analysis: Default::default(),
        review: review::ReviewState::default(),
        batch: Default::default(),
        game: None,
        coach: Default::default(),
        commentary: commentary::State::new(
            config.commentary_key_store.clone(),
            &settings.commentary,
        ),
        remote: crate::remote::RemoteSecrets::new(config.commentary_key_store.clone()),
        zhizi: crate::zhizi::ZhiziService::spawn(
            config.zhizi_client.clone(),
            config.commentary_key_store.clone(),
        ),
        startup_local_fallback: false,
        gtp: Default::default(),
        match_session: None,
        strength: None,
        summary_fingerprint: None,
        revision: 0,
        directory,
        cache: Some(cache),
        settings,
        pending_java_import: None,
        java_import_sequence: 0,
        config,
        running: None,
        trial: None,
        auto_setup: Default::default(),
        auto_curve: Default::default(),
        board_sync: Default::default(),
        benchmark: Default::default(),
        assets: Default::default(),
        acceleration: Default::default(),
        support: Some(support),
        updates: Default::default(),
        focus: None,
        output,
    };
    actor.restore_last_game().await;
    if let Err(error) = actor.publish_document() {
        actor.failed(error.to_string());
    }
    actor.start_initial_engine().await;
    actor.reconcile_comparison().await;
    actor.publish_sync();
    actor.publish_analysis_state();
    let period = actor
        .config
        .autosave_interval
        .max(std::time::Duration::from_millis(1));
    let mut autosave = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    autosave.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let web_deadline = actor.web_deadline();
        tokio::select! {
            _ = autosave.tick() => actor.autosave(false).await,
            () = std::future::ready(()), if actor.match_needs_advance() => actor.advance_match().await,
            () = std::future::ready(()), if actor.batch_needs_advance() => actor.advance_batch().await,
            command = commands.recv() => {
              let command = match command {
                Some(command) => match actor.web_trial_guard(command).and_then(|command| actor.acceleration_guard(command)).and_then(|command| actor.update_guard(command)).and_then(|command| actor.batch_guard(command)).and_then(|command| actor.match_guard(command)).and_then(|command| actor.game_guard(command)) { Some(command) => Some(command), None => continue },
                None => None,
              };
              match command {
                Some(Command::Capture(request)) => actor.capture_command(request),
                Some(Command::ComparisonState(reply)) => { let _ = reply.send(actor.comparison_view()); }
                Some(Command::WebBoard(request, reply)) => { let _ = reply.send(actor.web_board_command(request).await); }
                Some(Command::EnableAcceleration(backend, reply)) => actor.begin_acceleration(backend, reply).await,
                Some(Command::CancelAcceleration(reply)) => { let _ = reply.send(actor.cancel_asset_download().await); }
                Some(Command::AccelerationOutput(reply)) => { let _ = reply.send(actor.acceleration_output()); }
                Some(Command::SaveSlots(request)) => actor.save_slots_command(request).await,
                Some(Command::ZhiziService(reply)) => { let _ = reply.send(actor.zhizi.clone()); }
                Some(Command::ZhiziCredentialsChanged(reply)) => { let _ = reply.send(actor.zhizi_credentials_changed().await); }
                Some(Command::AnalysisTools(request)) => actor.analysis_tools_command(request).await,
                Some(Command::Scoring(request)) => actor.scoring_command(request).await,
                Some(Command::Remote(request)) => actor.remote_command(request).await,
                Some(Command::Commentary(request)) => actor.commentary_command(request).await,
                Some(Command::Coach(request)) => actor.coach_command(request).await,
                Some(Command::Gtp(request)) => actor.gtp_request(request).await,
                Some(Command::Match(request, reply)) => { let _ = reply.send(actor.match_command(request).await); }
                Some(Command::MatchView(reply)) => { let _ = reply.send(actor.match_view()); }
                Some(Command::Game(request, reply)) => { let _ = reply.send(actor.game_command(request).await); }
                Some(Command::GameView(reply)) => { let _ = reply.send(actor.game_view()); }
                Some(Command::SetAiPlayer(color, player, reply)) => { let _ = reply.send(actor.set_ai_player(color, player).await); }
                Some(Command::PlayBestMove(pv, reply)) => { let _ = reply.send(actor.play_best_move(pv).await); }
                Some(Command::Batch(request, reply)) => { let _ = reply.send(actor.batch_command(request).await); }
                Some(Command::BatchStatus(reply)) => { let _ = reply.send(actor.batch.status.clone()); }
                Some(Command::ClaimUpdateCheck(now, reply)) => { let _ = reply.send(actor.claim_update_check(now).await); }
                Some(Command::Support(reply)) => { let _ = reply.send(actor.support.clone().ok_or(AppError::Closed)); }
                Some(Command::Updates(reply)) => { let _ = reply.send(actor.updates.handle.clone().ok_or(crate::updates::UpdateError::SigningNotConfigured.into())); }
                Some(Command::UpdateStatus(reply)) => { let _ = reply.send(actor.updates.handle.as_ref().map(|handle| handle.status()).ok_or(crate::updates::UpdateError::SigningNotConfigured.into())); }
                Some(Command::AttachUpdates(handle, reply)) => { let _ = reply.send(actor.attach_updates(handle)); }
                Some(Command::Assets(reply)) => { let result = actor.asset_handle(); let _ = reply.send(result); }
                Some(Command::ReplaceMainEngine(expected, profile, restart, reply)) => actor.replace_main_engine(*expected, profile, restart, reply).await,
                Some(Command::DataDirectory(reply)) => { let _ = reply.send(actor.directory.clone()); }
                Some(Command::Engines(request, reply)) => { actor.engine_command(request, reply).await; },
                Some(Command::JavaImport(request)) => {
                    match request {
                        crate::java_import::Request::Preview(directory, reply) => { let _ = reply.send(actor.preview_java_import(directory).await); }
                        crate::java_import::Request::Confirm(id, reply) => { let _ = reply.send(actor.confirm_java_import(id).await); }
                    }
                }
                Some(Command::SetEngineSettings(settings, reply)) => { let _ = reply.send(actor.set_engine_settings(settings).await); }
                Some(Command::Windows(request)) => actor.window_request(request).await,
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
                Some(Command::Tencent(request)) => actor.tencent_request(request).await,
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
                Some(Command::ClipboardLink(reply)) => { let _ = reply.send(actor.clipboard_link().await); }
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
                    let web_result = actor.stop_web_board().await;
                    actor.autosave(true).await;
                    actor.stop_commentary();
                    if actor.batch_active() { actor.stop_batch(None).await; }
                    actor.shutdown_match().await;
                    actor.shutdown_gtp().await;
                    actor.auto_setup.cancel();
                    actor.cancel_acceleration(false).await;
                    actor.cancel_trial().await;
                    let updates_result = actor.updates.shutdown().await;
                    let assets_result = actor.assets.shutdown().await;
                    let benchmark_result = actor.stop_benchmark_process().await;
                    let sync_result = actor.stop_board_sync().await;
                    actor.cancel_comparison_trial().await;
                    let comparison_result = actor.stop_comparison().await;
                    let result = actor.stop_engine().await.and(sync_result).and(benchmark_result).and(assets_result).and(updates_result).and(web_result).and(comparison_result).map(|()| actor.revision);
                    if let Some(cache) = actor.cache.take() { cache.shutdown().await; }
                    if let Some(support) = actor.support.take() { let _ = support.shutdown().await; }
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
                None => { let _ = actor.stop_web_board().await; actor.autosave(true).await; if actor.batch_active() { actor.stop_batch(None).await; } actor.cancel_acceleration(false).await; actor.cancel_trial().await; let _ = actor.stop_benchmark_process().await; let _ = actor.stop_board_sync().await; let _ = actor.stop_engine().await; break; }
              }
            },
            event = web_board::receive(&mut actor.web_board) => actor.web_transport_event(event).await,
            () = web_board::deadline(web_deadline) => actor.expire_web_trial().await,
            progress = commentary::receive(&mut actor.commentary) => actor.commentary_progress(progress).await,
            line = gtp::receive(&mut actor.gtp) => actor.gtp_line(line),
            signal = next_signal(&mut actor.comparison.running) => actor.comparison_signal(signal).await,
            result = trial::wait(&mut actor.comparison.trial) => actor.comparison_trial_signal(result).await,
            signal = next_signal(&mut actor.running) => actor.cache_signal(signal).await,
            event = game::receive(&mut actor.game) => actor.game_signal(event).await,
            event = coach::receive(&mut actor.coach) => actor.coach_signal(event).await,
            result = review::receive(&mut actor.review) => actor.review_signal(result).await,
            result = trial::wait(&mut actor.trial) => actor.trial_signal(result).await,
            update = acceleration::receive(&mut actor.acceleration) => actor.acceleration_download(update).await,
            result = board_sync::receive(&mut actor.board_sync) => actor.board_sync_line(result).await,
            result = benchmark::receive(&mut actor.benchmark) => actor.benchmark_signal(result).await,
            progress = release::receive(&mut actor.updates) => actor.update_progress(progress).await,
            progress = weights::receive(&mut actor.assets) => actor.output.send(AppEvent::AssetDownloadProgress(progress)),
        }
        actor.sync_batch_progress().await;
        if !actor.updates.install_pending {
            actor.maybe_start_auto_curve().await;
        }
        actor.publish_analysis_state();
        actor.refresh_web_board();
    }
    actor.cancel_comparison_trial().await;
    let _ = actor.stop_comparison().await;
    actor.stop_commentary();
    actor.shutdown_match().await;
    actor.shutdown_gtp().await;
    let _ = actor.updates.shutdown().await;
    if let Some(support) = actor.support.take() {
        let _ = support.shutdown().await;
    }
    let _ = actor.assets.shutdown().await;
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
    async fn apply(&mut self, input: Input) -> Result<u64, AppError> {
        match input {
            Input::SetCaptureSettings(settings) => self.set_capture_settings(settings).await?,
            Input::FinishCapture(id, result) => self.finish_capture(id, result).await?,
            Input::DismissAccelerationHint => self.dismiss_acceleration_hint().await?,
            Input::EnsureNoBatch => self.ensure_no_batch()?,
            Input::SetFlashAnalysis(settings) => self.set_flash_analysis(settings).await?,
            Input::StartTrial => self.start_move_trial().await?,
            Input::EndTrial => self.end_move_trial().await?,
            Input::GenerateTsumego(options) => self.generate_tsumego(options).await?,
            Input::ExitTsumego => self.exit_tsumego().await?,
            Input::OpenPath(path) => self.open_path(path).await?,
            Input::LoadSlot(id) => self.load_slot(&id, false).await?,
            Input::SetAutosave(enabled) => self.set_save_setting(enabled, false).await?,
            Input::SetRestoreLastGame(enabled) => self.set_save_setting(enabled, true).await?,
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
            Input::CopySgf => self.copy_sgf().await?,
            Input::PasteSgf => self.paste_sgf().await?,
            Input::SaveCleanSgf(path, comments, branch) => {
                self.save_clean_sgf(path, comments, branch).await?
            }
            Input::SavePng(path, bytes) => self.save_png(path, bytes).await?,
            Input::CopyPng(bytes) => self.copy_png(bytes).await?,
            Input::NewGame(options) => self.new_game(options).await?,
            Input::ConvertToStartPosition => self.convert_to_start_position().await?,
            Input::ClearRecentFiles => self.set_recent_files(Vec::new()).await?,
            Input::Edit(command) => {
                if let document::Command::Play(mv) = command
                    && self.game_playing()
                {
                    return self.game_human_move(mv).await;
                }
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
                let transformed_saved = matches!(command, document::Command::Transform(_))
                    .then(|| self.valid_all_saved_nodes());
                let before = self.document.revision();
                let before_content = self.document.content_revision();
                self.document.apply(command)?;
                if self.document.content_revision() != before_content {
                    if let Some(nodes) = &transformed_saved {
                        self.file_state
                            .analysis_edits
                            .document_transform(nodes.clone());
                        self.import_transformed_saved_analysis(nodes)?;
                    } else {
                        self.file_state.analysis_edits.document_edit();
                    }
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
            Input::SetLanguage(language) => self.set_language(language).await?,
            Input::SetAlwaysOnTop(enabled) => self.set_always_on_top(enabled).await?,
            Input::SetUiScale(scale) => self.set_ui_scale(scale).await?,
            Input::SetPlaySound(enabled) => self.set_play_sound(enabled).await?,
            Input::SetAutoCheckUpdates(enabled) => self.set_auto_check_updates(enabled).await?,
            Input::SetAnalysisExtras(extras) => self.set_analysis_extras(extras).await?,
            Input::SetPureNetwork(enabled) => self.set_pure_network(enabled).await?,
            Input::SetPondering(enabled) => {
                self.set_pondering(enabled).await?;
            }
            Input::SetAutoQuickAnalyzeOnLoad(enabled) => {
                self.set_auto_quick_analyze_on_load(enabled).await?;
            }
            Input::ConfigureEngine(profile) => self.configure_engine(profile).await?,
            Input::PrepareUpdateInstall => self.prepare_update_install().await?,
            Input::RecoverUpdateInstall => self.recover_update_install().await,
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
        self.document_replaced_with_analysis(cached, import_saved, None)
            .await
    }

    async fn document_replaced_with_analysis(
        &mut self,
        cached: HashMap<AnalysisKey, crate::StoredAnalysis>,
        import_saved: bool,
        comparison: Option<comparison::DocumentState>,
    ) -> Result<(), AppError> {
        self.restriction = None;
        self.restricted_analysis = None;
        self.scoring = None;
        self.reset_commentary();
        self.invalidate_game_document();
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
        self.reset_comparison_document().await;
        if let Some(saved) = comparison {
            self.restore_comparison_document(saved);
        }
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
        self.exit_scoring_for_document_change().await?;
        self.revision = self.next_revision()?;
        self.output.clear_analysis();
        self.publish_document()?;
        self.update_focus_with_resume(resume_limited).await;
        Ok(())
    }

    fn set_summary_fingerprint(&mut self, fingerprint: Option<String>) {
        let fingerprint = match fingerprint
            .map(|value| self.evaluation_fingerprint(&value))
            .transpose()
        {
            Ok(fingerprint) => fingerprint,
            Err(error) => {
                self.failed(error.to_string());
                return;
            }
        };
        if self.summary_fingerprint != fingerprint {
            self.summary_fingerprint = fingerprint;
            let saved = self
                .review
                .results
                .iter()
                .filter(|(_, result)| {
                    result.saved.is_some() && self.settings.search_params.is_none()
                })
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
        self.cancel_game_query().await?;
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
        self.acceleration_output();
        self.acceleration.monitor = None;
        self.output
            .send(AppEvent::EngineStatusChanged(EngineStatus::Stopped));
        Ok(())
    }
}

#[cfg(test)]
mod tests;
