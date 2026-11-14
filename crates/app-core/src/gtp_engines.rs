//! Third-party engine settings and console IPC types.
use crate::{AppCore, AppError, Settings, SettingsError, actor::Command};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct GtpEngineEntry {
    #[ts(as = "Option<String>", optional)]
    pub id: String,
    #[ts(as = "Option<String>", optional)]
    pub name: String,
    #[ts(as = "Option<PathBuf>", optional)]
    pub program: PathBuf,
    #[ts(as = "Option<Vec<String>>", optional)]
    pub args: Vec<String>,
    #[ts(optional = nullable)]
    pub working_dir: Option<PathBuf>,
    #[ts(as = "Option<Vec<String>>", optional)]
    pub init_commands: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "type", content = "data")]
pub enum GtpError {
    #[error("unknown GTP engine: {0}")]
    UnknownEngine(String),
    #[error("GTP engine is in use: {0}")]
    Busy(String),
    #[error("command {command} changes the active game")]
    StateChangingCommand { command: String },
    #[error("invalid GTP command: {0}")]
    InvalidCommand(String),
    #[error("GTP engine {engine_id}: {reason}")]
    Session { engine_id: String, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GtpEngineInfo {
    #[ts(as = "Option<String>", optional)]
    pub name: String,
    #[ts(as = "Option<String>", optional)]
    pub version: String,
    #[ts(as = "Option<Vec<String>>", optional)]
    pub commands: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum GtpConsoleDirection {
    Sent,
    Received,
    Stderr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GtpConsoleLine {
    #[ts(as = "Option<String>", optional)]
    pub engine_id: String,
    #[ts(as = "Option<f64>", optional)]
    pub sequence: u64,
    #[ts(as = "Option<GtpConsoleDirection>", optional)]
    pub direction: GtpConsoleDirection,
    #[ts(as = "Option<String>", optional)]
    pub text: String,
}

pub(crate) enum Request {
    Add(GtpEngineEntry, oneshot::Sender<Result<Settings, AppError>>),
    Update(GtpEngineEntry, oneshot::Sender<Result<Settings, AppError>>),
    Remove(String, oneshot::Sender<Result<Settings, AppError>>),
    Test(String, oneshot::Sender<Result<GtpEngineInfo, AppError>>),
    Console(
        String,
        oneshot::Sender<Result<Vec<GtpConsoleLine>, AppError>>,
    ),
    Send(String, String, oneshot::Sender<Result<String, AppError>>),
}

impl AppCore {
    pub async fn add_gtp_engine(&self, entry: GtpEngineEntry) -> Result<Settings, AppError> {
        self.gtp_request(|reply| Request::Add(entry, reply)).await
    }
    pub async fn update_gtp_engine(&self, entry: GtpEngineEntry) -> Result<Settings, AppError> {
        self.gtp_request(|reply| Request::Update(entry, reply))
            .await
    }
    pub async fn remove_gtp_engine(&self, id: String) -> Result<Settings, AppError> {
        self.gtp_request(|reply| Request::Remove(id, reply)).await
    }
    pub async fn test_gtp_engine(&self, id: String) -> Result<GtpEngineInfo, AppError> {
        self.gtp_request(|reply| Request::Test(id, reply)).await
    }
    pub async fn gtp_console(&self, engine_id: String) -> Result<Vec<GtpConsoleLine>, AppError> {
        self.gtp_request(|reply| Request::Console(engine_id, reply))
            .await
    }
    pub async fn gtp_console_send(
        &self,
        engine_id: String,
        text: String,
    ) -> Result<String, AppError> {
        self.gtp_request(|reply| Request::Send(engine_id, text, reply))
            .await
    }
    async fn gtp_request<T>(
        &self,
        request: impl FnOnce(oneshot::Sender<Result<T, AppError>>) -> Request,
    ) -> Result<T, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Gtp(request(reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

pub(crate) fn validate_entries(entries: &[GtpEngineEntry]) -> Result<(), SettingsError> {
    let mut ids = HashSet::new();
    for entry in entries {
        if entry.id.trim().is_empty()
            || entry.name.trim().is_empty()
            || !ids.insert(&entry.id)
            || entry.program.as_os_str().is_empty()
            || entry.args.iter().any(|arg| arg.contains('\0'))
            || entry
                .working_dir
                .as_ref()
                .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err(crate::engines::invalid(
                "GTP IDs must be unique; ID, name and program must be nonempty",
            ));
        }
        for line in &entry.init_commands {
            command_name(line).map_err(|error| crate::engines::invalid(&error.to_string()))?;
        }
    }
    Ok(())
}

/// Use the same parser for stored initialization and console wire commands.
pub(crate) fn command_name(text: &str) -> Result<String, GtpError> {
    gtp::Command::parse(text)
        .map(|command| command.name.to_ascii_lowercase())
        .map_err(|error| GtpError::InvalidCommand(error.to_string()))
}

pub(crate) fn changes_game(name: &str) -> bool {
    matches!(
        name,
        "play"
            | "genmove"
            | "undo"
            | "clear_board"
            | "boardsize"
            | "komi"
            | "fixed_handicap"
            | "place_free_handicap"
            | "set_free_handicap"
            | "loadsgf"
            | "quit"
    )
}
