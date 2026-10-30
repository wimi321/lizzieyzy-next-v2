//! Headless application entry point. A single actor owns the document, settings,
//! engine session and analysis coordinator. Handles contain only channels.

mod actor;
mod delivery;
pub mod engine_setup;
mod engine_trial;
mod graph;
mod inspection;
mod review;
mod settings;
mod storage;
mod view;

pub use delivery::AppSubscription;
pub use engine_trial::{EngineSetupError, EngineSetupProfile};
pub use graph::{AnalysisSummary, GraphPointView, GraphView};
pub use inspection::{
    GameInfoView, PreviewStoneView, VariationNodeView, VariationPreviewView, VariationTreeView,
};
pub use review::*;
pub use settings::{EngineProfile, Settings, SettingsError};
pub use view::*;

use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
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
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error(transparent)]
    Analysis(#[from] analysis::AnalysisError),
    #[error(transparent)]
    Session(#[from] engine_session::SessionError),
    #[error(transparent)]
    EngineSetup(#[from] EngineSetupError),
    #[error(transparent)]
    WholeGame(#[from] WholeGameError),
}

/// Runtime tuning, separate from the versioned user settings. Defaults use the
/// coordinator/session defaults, including the long first-start deadline.
#[derive(Clone, Debug)]
pub struct AppConfig {
    pub session: engine_session::SessionConfig,
    pub analysis: analysis::AnalysisConfig,
    pub engine_setup_timeout: std::time::Duration,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            session: Default::default(),
            analysis: Default::default(),
            engine_setup_timeout: std::time::Duration::from_secs(120),
        }
    }
}

#[derive(Clone)]
pub struct AppCore {
    commands: mpsc::Sender<actor::Command>,
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
    let settings = Settings::load(&directory).await?;
    let (commands, receiver) = mpsc::channel(64);
    let (output, subscription) = delivery::channel();
    tokio::spawn(actor::run(directory, settings, config, receiver, output));
    Ok((AppCore { commands }, subscription))
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
