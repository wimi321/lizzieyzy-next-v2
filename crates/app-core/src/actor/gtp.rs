use super::Actor;
use crate::{
    AppError, AppEvent, GtpConsoleDirection, GtpConsoleLine, GtpEngineInfo, GtpError, Settings,
    gtp_engines::{Request, changes_game, command_name},
};
use engine_session::{
    TransportSpec,
    gtp::{GtpHandle, GtpLogLine},
};
use std::collections::BTreeMap;
use tokio::sync::mpsc;

struct Running {
    handle: GtpHandle,
    forward: tokio::task::JoinHandle<()>,
    testing: Option<tokio::task::JoinHandle<()>>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.forward.abort();
        if let Some(task) = &self.testing {
            task.abort();
        }
    }
}

pub(super) struct State {
    sessions: BTreeMap<String, Running>,
    archived: BTreeMap<String, Vec<GtpConsoleLine>>,
    sender: mpsc::Sender<GtpConsoleLine>,
    receiver: mpsc::Receiver<GtpConsoleLine>,
}
impl Default for State {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel(2000);
        Self {
            sessions: BTreeMap::new(),
            archived: BTreeMap::new(),
            sender,
            receiver,
        }
    }
}
pub(super) async fn receive(state: &mut State) -> Option<GtpConsoleLine> {
    state.receiver.recv().await
}
fn failure(id: &str, error: impl std::fmt::Display) -> AppError {
    GtpError::Session {
        engine_id: id.into(),
        reason: error.to_string(),
    }
    .into()
}
fn line(id: &str, value: GtpLogLine) -> GtpConsoleLine {
    use engine_session::gtp::GtpDirection;
    GtpConsoleLine {
        engine_id: id.into(),
        sequence: value.sequence,
        text: value.text,
        direction: match value.direction {
            GtpDirection::Sent => GtpConsoleDirection::Sent,
            GtpDirection::Received => GtpConsoleDirection::Received,
            GtpDirection::Stderr => GtpConsoleDirection::Stderr,
        },
    }
}

impl Actor {
    fn gtp_busy(&self, id: &str) -> bool {
        self.game_gtp_engine_busy(id) || self.match_gtp_engine_busy(id) || self.gtp_testing(id)
    }
    fn gtp_testing(&self, id: &str) -> bool {
        self.gtp
            .sessions
            .get(id)
            .and_then(|session| session.testing.as_ref())
            .is_some_and(|task| !task.is_finished())
    }
    pub(super) async fn get_gtp_handle(&mut self, id: &str) -> Result<GtpHandle, AppError> {
        let handle = self.ensure_gtp_handle(id).await?;
        handle.ready().await.map_err(|error| failure(id, error))?;
        Ok(handle)
    }
    async fn ensure_gtp_handle(&mut self, id: &str) -> Result<GtpHandle, AppError> {
        if self.gtp_testing(id) {
            return Err(GtpError::Busy(id.into()).into());
        }
        if let Some(running) = self.gtp.sessions.get(id) {
            let handle = running.handle.clone();
            if let engine_session::gtp::GtpStatus::Failed(error) = &*handle.status().borrow()
                && self.gtp_busy(id)
            {
                return Err(failure(id, error));
            }
            if matches!(
                *handle.status().borrow(),
                engine_session::gtp::GtpStatus::Starting | engine_session::gtp::GtpStatus::Ready(_)
            ) {
                return Ok(handle);
            }
            self.close_gtp_engine(id).await;
        }
        let entry = self
            .settings
            .gtp_engines
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| GtpError::UnknownEngine(id.into()))?;
        let spec = TransportSpec::LocalProcess {
            program: entry.program.clone(),
            args: entry.args.iter().map(Into::into).collect(),
            cwd: entry.working_dir.clone(),
            env: BTreeMap::new(),
        };
        let mut config = self.config.gtp_session.clone();
        config.init_commands = entry.init_commands.clone();
        let handle = engine_session::gtp::spawn(spec, config);
        let mut logs = handle.subscribe();
        let sender = self.gtp.sender.clone();
        let engine_id = id.to_owned();
        let forward = tokio::spawn(async move {
            loop {
                match logs.recv().await {
                    Ok(value) => {
                        let _ = sender.try_send(line(&engine_id, value));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        self.gtp.archived.remove(id);
        self.gtp.sessions.insert(
            id.into(),
            Running {
                handle: handle.clone(),
                forward,
                testing: None,
            },
        );
        Ok(handle)
    }
    pub(super) async fn close_gtp_engine(&mut self, id: &str) {
        if let Some(running) = self.gtp.sessions.remove(id) {
            let _ = running.handle.clone().shutdown().await;
            if let Ok(lines) = running.handle.console().await {
                self.gtp.archived.insert(
                    id.into(),
                    lines.into_iter().map(|value| line(id, value)).collect(),
                );
            }
        }
    }
    pub(super) async fn shutdown_gtp(&mut self) {
        let ids: Vec<_> = self.gtp.sessions.keys().cloned().collect();
        for id in ids {
            self.close_gtp_engine(&id).await;
        }
    }
    pub(super) async fn gtp_request(&mut self, request: Request) {
        match request {
            Request::Add(entry, reply) => {
                let _ = reply.send(self.edit_gtp(Some(entry), None).await);
            }
            Request::Update(entry, reply) => {
                let id = entry.id.clone();
                let _ = reply.send(self.edit_gtp(Some(entry), Some(id)).await);
            }
            Request::Remove(id, reply) => {
                let _ = reply.send(self.edit_gtp(None, Some(id)).await);
            }
            Request::Test(id, reply) => self.test_gtp(id, reply).await,
            Request::Console(id, reply) => {
                let _ = reply.send(self.read_gtp_console(&id).await);
            }
            Request::Send(id, text, reply) => match self.queue_gtp_console(&id, text).await {
                Ok(response) => {
                    tokio::spawn(async move {
                        let _ = reply.send(
                            response
                                .recv()
                                .await
                                .map(|response| response.body)
                                .map_err(|error| failure(&id, error)),
                        );
                    });
                }
                Err(error) => {
                    let _ = reply.send(Err(error));
                }
            },
        }
    }
    async fn edit_gtp(
        &mut self,
        mut entry: Option<crate::GtpEngineEntry>,
        existing: Option<String>,
    ) -> Result<Settings, AppError> {
        let mut candidate = self.settings.clone();
        if let Some(id) = &existing {
            if self.gtp_busy(id) {
                return Err(GtpError::Busy(id.clone()).into());
            }
            let index = candidate
                .gtp_engines
                .iter()
                .position(|entry| &entry.id == id)
                .ok_or_else(|| GtpError::UnknownEngine(id.clone()))?;
            candidate.gtp_engines.remove(index);
            if let Some(entry) = entry.take() {
                candidate.gtp_engines.insert(index, entry);
            }
        } else if let Some(mut entry) = entry {
            if entry.id.is_empty() {
                let mut n = 1_u64;
                while candidate
                    .gtp_engines
                    .iter()
                    .any(|entry| entry.id == format!("gtp-{n}"))
                {
                    n += 1;
                }
                entry.id = format!("gtp-{n}");
            }
            candidate.gtp_engines.push(entry);
        }
        candidate.save(&self.directory).await?;
        if let Some(id) = &existing {
            self.close_gtp_engine(id).await;
        }
        self.settings = candidate;
        self.gtp.archived.retain(|id, _| {
            self.settings
                .gtp_engines
                .iter()
                .any(|entry| &entry.id == id)
        });
        Ok(self.settings.clone())
    }
    async fn test_gtp(
        &mut self,
        id: String,
        reply: tokio::sync::oneshot::Sender<Result<GtpEngineInfo, AppError>>,
    ) {
        if self.gtp_busy(&id) {
            let _ = reply.send(Err(GtpError::Busy(id).into()));
            return;
        }
        self.close_gtp_engine(&id).await;
        let handle = match self.ensure_gtp_handle(&id).await {
            Ok(handle) => handle,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        let task_id = id.clone();
        let task = tokio::spawn(async move {
            let result = async {
                let info = handle
                    .ready()
                    .await
                    .map_err(|error| failure(&task_id, error))?;
                handle
                    .command("quit")
                    .await
                    .map_err(|error| failure(&task_id, error))?;
                Ok(GtpEngineInfo {
                    name: info.name,
                    version: info.version,
                    commands: info.commands,
                })
            }
            .await;
            let shutdown = handle
                .shutdown()
                .await
                .map_err(|error| failure(&task_id, error));
            let _ = reply.send(result.and_then(|info| shutdown.map(|()| info)));
        });
        if let Some(running) = self.gtp.sessions.get_mut(&id) {
            running.testing = Some(task);
        }
    }
    async fn read_gtp_console(&self, id: &str) -> Result<Vec<GtpConsoleLine>, AppError> {
        if let Some(running) = self.gtp.sessions.get(id) {
            return running
                .handle
                .console()
                .await
                .map(|lines| lines.into_iter().map(|value| line(id, value)).collect())
                .map_err(|error| failure(id, error));
        }
        if !self.settings.gtp_engines.iter().any(|entry| entry.id == id) {
            return Err(GtpError::UnknownEngine(id.into()).into());
        }
        Ok(self.gtp.archived.get(id).cloned().unwrap_or_default())
    }
    async fn queue_gtp_console(
        &mut self,
        id: &str,
        text: String,
    ) -> Result<engine_session::gtp::GtpReply, AppError> {
        let name = command_name(&text)?;
        if self.gtp_busy(id) && changes_game(&name) {
            return Err(GtpError::StateChangingCommand { command: name }.into());
        }
        let handle = self.ensure_gtp_handle(id).await?;
        handle
            .enqueue_command(text)
            .map_err(|error| failure(id, error))
    }
    pub(super) fn gtp_line(&self, value: Option<GtpConsoleLine>) {
        if let Some(value) = value {
            self.output.send(AppEvent::GtpConsoleLine(value));
        }
    }
}
