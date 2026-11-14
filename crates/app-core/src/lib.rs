//! Headless application entry point. A single actor owns the document, settings,
//! engine session and analysis coordinator. Handles contain only channels.

mod actor;
mod analysis_cache;
mod analysis_extras;
mod analysis_limit;
pub mod assets;
mod auto_setup;
mod batch;
mod benchmark;
mod board_sync;
mod coach;
mod coach_api;
mod commentary;
pub mod commentary_client;
pub mod commentary_keys;
mod delivery;
mod diagnostics;
mod display;
mod edit;
mod engine_display_name;
mod engine_settings;
pub mod engine_setup;
mod engine_trial;
mod engines;
mod everyday;
mod flash_analysis;
pub mod fox;
mod game;
mod game_api;
mod graph;
mod gtp_engines;
mod inspection;
mod java_import;
mod match_game;
mod move_editing;
mod move_rank;
mod preferences;
mod problem_moves;
mod release;
mod remote;
mod remote_link;
mod review;
mod runtime;
mod settings;
mod setup_markup;
mod storage;
mod strength_estimate;
pub mod support;
pub mod tencent;
mod tsumego;
pub mod updates;
mod view;
mod weights;
pub mod zhizi;
mod zhizi_api;
mod zhizi_engine;

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_support;

pub use analysis_extras::AnalysisExtras;
pub use analysis_limit::{AnalysisLimitSettings, AnalysisPauseReason, AnalysisState};
pub use assets::{
    AccelerationPlan, AccelerationReason, AssetDownloadStatus, AssetError, DownloadPhase,
    HumanSlModelStatus, OfficialWeight,
};
pub use auto_setup::{AutoSetupAttempt, AutoSetupError};
pub use batch::*;
pub use benchmark::*;
pub use board_sync::{
    BoardSyncError, BoardSyncLanguage, BoardSyncOptions, BoardSyncPhase, BoardSyncStatus,
};
pub use coach::*;
pub use commentary::*;
pub use commentary_keys::CommentaryKeyStatus;
pub use delivery::AppSubscription;
pub use diagnostics::{
    DiagnosticCheck, DiagnosticKind, DiagnosticRepair, DiagnosticStatus, EngineDiagnostics,
    RepairResources,
};
pub use display::{
    DisplaySettings, DisplaySettingsError, EstimateLocation, EstimateStyle, MoveNumberMode,
    MoveRankMarkMode, NextMoveHint, SubBoardHeat, TopToolbarLayout, WinrateGraphMode,
};
pub use edit::{GameInfoEdit, NewGameOptions};
pub use engine_display_name::{EngineDisplayName, engine_display_name};
pub use engine_settings::{EngineCustomRules, EngineRules, EngineSettings};
pub use engine_trial::{EngineSetupError, EngineSetupProfile};
pub use engines::EngineEntry;
pub use everyday::{
    BoardTransform, Clipboard, ClipboardError, ClipboardFuture, ClipboardImage, MAX_PNG_BYTES,
    PngError,
};
pub use flash_analysis::FlashAnalysisSettings;
pub use game::*;
pub use graph::{AnalysisSummary, GraphPointView, GraphView};
pub use gtp_engines::*;
pub use inspection::{
    GameInfoView, PreviewStoneView, VariationNodeView, VariationPreviewView, VariationTreeView,
};
pub use java_import::{JavaImportPreview, JavaImportedEngine};
pub use match_game::*;
pub use move_editing::{StoneOriginView, TrialError};
pub use move_rank::*;
pub use preferences::UiLanguage;
pub use problem_moves::*;
pub use remote::{
    RemoteEngineConfig, RemoteEngineError, RemoteEngineInput, RemoteSshAuth, SshAuthConfig,
};
pub use remote_link::{RemoteLinkError, normalize_remote_link, read_remote_link_qr};
pub use review::*;
pub use settings::{EngineProfile, Settings, SettingsError};
pub use setup_markup::{MarkupKindView, MarkupView};
pub use strength_estimate::*;
pub use support::FullLogStatus;
pub use tsumego::*;
pub use updates::{UpdateCheck, UpdateDownloadStatus};
pub use view::*;
pub use zhizi_engine::{ZhiziEngineOptions, ZhiziPlan, ZhiziWeight};

use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Zhizi(#[from] zhizi::ZhiziError),
    #[error(transparent)]
    Remote(#[from] RemoteEngineError),
    #[error(transparent)]
    RemoteLink(#[from] RemoteLinkError),
    #[error(transparent)]
    Commentary(#[from] CommentaryError),
    #[error(transparent)]
    Coach(#[from] CoachError),
    #[error(transparent)]
    Gtp(#[from] GtpError),
    #[error(transparent)]
    Match(#[from] MatchError),
    #[error("game is running")]
    GameRunning,
    #[error(transparent)]
    Game(#[from] GameError),
    #[error("pure network is active")]
    PureNetworkActive,
    #[error("whole-game analysis is running or paused")]
    WholeGameAnalysisActive,
    #[error("batch analysis is running or paused")]
    BatchAnalysisRunning,
    #[error(transparent)]
    Batch(#[from] BatchError),
    #[error(transparent)]
    Clipboard(#[from] ClipboardError),
    #[error(transparent)]
    Png(#[from] PngError),
    #[error(transparent)]
    Support(#[from] support::SupportError),
    #[error(transparent)]
    Update(#[from] updates::UpdateError),
    #[error(transparent)]
    Asset(#[from] AssetError),
    #[error(transparent)]
    Trial(#[from] TrialError),
    #[error(transparent)]
    Benchmark(#[from] BenchmarkError),
    #[error("application core is closed")]
    Closed,
    #[error("application revision is exhausted")]
    RevisionLimit,
    #[error("SGF collection contains no games")]
    EmptyCollection,
    #[error(transparent)]
    Document(#[from] document::DocError),
    #[error(transparent)]
    Sgf(#[from] sgf::SgfError),
    #[error(transparent)]
    Lz(#[from] document::LzError),
    #[error("GIB files are import-only; choose an SGF save target")]
    GibSaveTarget,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error("file saved to {path}, but recent files could not be saved: {source}")]
    RecentFilesAfterSave {
        path: PathBuf,
        #[source]
        source: SettingsError,
    },
    #[error(transparent)]
    Analysis(#[from] analysis::AnalysisError),
    #[error(transparent)]
    Session(#[from] engine_session::SessionError),
    #[error(transparent)]
    EngineSetup(#[from] EngineSetupError),
    #[error(transparent)]
    AutoSetup(#[from] AutoSetupError),
    #[error(transparent)]
    BoardSync(#[from] BoardSyncError),
    #[error(transparent)]
    Fox(#[from] fox::FoxError),
    #[error(transparent)]
    Tencent(#[from] tencent::TencentError),
    #[error(transparent)]
    OpenUrl(#[from] tencent::OpenUrlError),
    #[error(transparent)]
    WholeGame(#[from] WholeGameError),
    #[error(transparent)]
    MoveRank(#[from] MoveRankError),
    #[error(transparent)]
    Strength(#[from] strength::model::ModelError),
    #[error(transparent)]
    Tsumego(#[from] TsumegoError),
}

/// Runtime tuning, separate from the versioned user settings. Defaults use the
/// coordinator/session defaults, including the long first-start deadline.
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// Constructed lazily; tests inject a loopback HTTP endpoint.
    pub zhizi_client: Option<zhizi::ZhiziClient>,
    /// Shared secret-store adapter; the P3-16 field name remains source compatible.
    pub commentary_key_store: Option<std::sync::Arc<dyn commentary_keys::SecretStore>>,
    pub games_directory: Option<PathBuf>,
    pub game_random_seed: Option<u64>,
    pub gtp_session: engine_session::gtp::GtpSessionConfig,
    pub game_query_timeout: std::time::Duration,
    pub batch_clock: std::sync::Arc<dyn BatchClock>,
    /// Native host adapter; headless callers and tests never access the system clipboard.
    pub clipboard: Option<std::sync::Arc<dyn Clipboard>>,
    #[cfg(test)]
    pub(crate) runtime_platform: bool,
    #[cfg(test)]
    pub(crate) asset_catalog: Option<assets::AssetCatalog>,
    pub session: engine_session::SessionConfig,
    pub analysis: analysis::AnalysisConfig,
    /// Defaults to the settings directory for headless callers.
    pub cache_directory: Option<PathBuf>,
    pub cache_max_bytes: u64,
    pub engine_setup_timeout: std::time::Duration,
    /// Optional client with caller-provided endpoints and timeout. None uses
    /// the public Fox endpoints, constructed lazily on the first Fox command.
    pub fox_client: Option<fox::FoxClient>,
    /// Constructed lazily, so optional networking cannot prevent startup.
    pub tencent_client: Option<tencent::TencentClient>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            zhizi_client: None,
            commentary_key_store: None,
            games_directory: None,
            game_random_seed: None,
            gtp_session: Default::default(),
            game_query_timeout: std::time::Duration::from_secs(120),
            batch_clock: std::sync::Arc::new(LocalBatchClock),
            clipboard: None,
            #[cfg(test)]
            runtime_platform: runtime::supported(),
            #[cfg(test)]
            asset_catalog: None,
            session: Default::default(),
            analysis: Default::default(),
            cache_directory: None,
            cache_max_bytes: analysis::persistent_cache::DEFAULT_MAX_BYTES,
            engine_setup_timeout: std::time::Duration::from_secs(120),
            fox_client: None,
            tencent_client: None,
        }
    }
}

#[derive(Clone)]
pub struct AppCore {
    commands: mpsc::Sender<actor::Command>,
    #[cfg(test)]
    runtime_platform: bool,
}

/// Load settings from the caller's directory and start on the current runtime.
/// A bad engine profile is reported as an event; it does not prevent editing.
pub async fn spawn(
    settings_directory: impl Into<PathBuf>,
) -> Result<(AppCore, AppSubscription), AppError> {
    spawn_with_config(settings_directory, AppConfig::default()).await
}

pub async fn spawn_with_config(
    settings_directory: impl Into<PathBuf>,
    config: AppConfig,
) -> Result<(AppCore, AppSubscription), AppError> {
    let directory = settings_directory.into();
    let mut settings = Settings::load(&directory).await?;
    if settings.games_directory.is_none() {
        settings.games_directory = Some(
            config
                .games_directory
                .clone()
                .unwrap_or_else(|| directory.join("MyGames")),
        );
    }
    let support = support::spawn(
        config
            .cache_directory
            .clone()
            .unwrap_or_else(|| directory.clone()),
    )
    .await?;
    let (commands, receiver) = mpsc::channel(64);
    let (output, subscription) = delivery::channel();
    #[cfg(test)]
    let runtime_platform = config.runtime_platform;
    tokio::spawn(actor::run(
        directory, settings, config, receiver, output, support,
    ));
    Ok((
        AppCore {
            commands,
            #[cfg(test)]
            runtime_platform,
        },
        subscription,
    ))
}

impl AppCore {
    /// Opens the first game in an SGF collection. Completion acknowledges the
    /// document event and focus transition, not completion of engine analysis.
    pub async fn open_bytes(&self, bytes: Vec<u8>) -> Result<u64, AppError> {
        self.send(actor::Input::OpenBytes(bytes)).await
    }

    pub async fn open_path(&self, path: impl Into<PathBuf>) -> Result<u64, AppError> {
        self.send(actor::Input::OpenPath(path.into())).await
    }

    /// Open the first readable SGF in native drop order, as one actor command.
    /// Unsupported extensions are ignored; failures are published as OpenFailed.
    pub async fn open_dropped_paths(&self, paths: Vec<PathBuf>) -> Result<u64, AppError> {
        self.send(actor::Input::OpenDroppedPaths(paths)).await
    }

    /// Writes an adjacent temporary file and replaces the target on success.
    pub async fn save(&self, path: impl Into<PathBuf>) -> Result<(), AppError> {
        self.send(actor::Input::Save(path.into())).await.map(|_| ())
    }

    pub async fn apply(&self, command: document::Command) -> Result<u64, AppError> {
        self.send(actor::Input::Edit(command)).await
    }

    pub async fn play(&self, mv: go_core::Move) -> Result<u64, AppError> {
        self.apply(document::Command::Play(mv)).await
    }

    pub async fn step(&self, count: i32) -> Result<u64, AppError> {
        self.apply(document::Command::Step(count)).await
    }

    pub async fn go_to(&self, node_id: u32) -> Result<u64, AppError> {
        self.apply(document::Command::GoTo(document::NodeId(node_id)))
            .await
    }

    pub async fn undo(&self) -> Result<u64, AppError> {
        self.send(actor::Input::Undo).await
    }

    pub async fn redo(&self) -> Result<u64, AppError> {
        self.send(actor::Input::Redo).await
    }

    /// Persist the enabled state. Pausing also invalidates unread analysis.
    pub async fn set_pondering(&self, enabled: bool) -> Result<(), AppError> {
        self.send(actor::Input::SetPondering(enabled))
            .await
            .map(|_| ())
    }

    /// Persist and activate one profile (None disables the engine). Invalid
    /// files produce Failed events while the saved profile remains editable.
    pub async fn configure_engine(&self, profile: Option<EngineProfile>) -> Result<(), AppError> {
        self.send(actor::Input::ConfigureEngine(profile))
            .await
            .map(|_| ())
    }

    /// Keep the current engine/settings until the candidate reaches Ready.
    /// The caller supplies the bundled template path; AppCore has no shell dependency.
    pub async fn try_configure_engine(
        &self,
        profile: EngineSetupProfile,
        default_config_source: PathBuf,
    ) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::TryConfigureEngine(
                profile,
                default_config_source,
                reply,
            ))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    pub async fn settings(&self) -> Result<Settings, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::Settings(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }

    pub async fn auto_quick_analyze_on_load(&self) -> Result<bool, AppError> {
        Ok(self.settings().await?.auto_quick_analyze_on_load)
    }

    /// Persist before applying. Enabling affects the next successful game load.
    pub async fn set_auto_quick_analyze_on_load(&self, enabled: bool) -> Result<bool, AppError> {
        self.send(actor::Input::SetAutoQuickAnalyzeOnLoad(enabled))
            .await?;
        Ok(enabled)
    }

    /// Cancels analysis, reaps the engine, and then closes the event stream.
    pub async fn shutdown(self) -> Result<(), AppError> {
        self.send(actor::Input::Shutdown).await.map(|_| ())
    }

    async fn send(&self, input: actor::Input) -> Result<u64, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::Apply(input, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}
