//! The application actor is the sole owner of viewers, credentials and trials.
mod guard;
mod snapshots;
mod trial;

use super::Actor;
use crate::{
    AppError, AppEvent, WebBoardStatus, WebServerMessage, WebTrialOwner,
    web_board::{
        EmptyAssets,
        connection::{ConnectionChannels, Outbound, TransportEvent},
        security, server,
    },
    web_board_api::Request,
};
use std::{
    collections::{HashMap, VecDeque},
    net::Ipv4Addr,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::Instant,
};

#[derive(Default)]
pub(super) struct State {
    pub trial: Option<Trial>,
    service: Option<Service>,
    clients: HashMap<u64, Client>,
    sequence: u64,
    last_status: WebBoardStatus,
    stamp: Option<(u64, Option<u64>)>,
    snapshot: Option<crate::WebFullState>,
    history: Option<crate::GraphView>,
    problems: Option<crate::ProblemMovesView>,
    trial_view: Option<crate::WebTrialState>,
}

struct Service {
    port: u16,
    addresses: Vec<String>,
    events: mpsc::Receiver<TransportEvent>,
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
}

struct Client {
    id: String,
    reconnect: String,
    outgoing: watch::Sender<Outbound>,
    messages: VecDeque<Instant>,
}

pub(super) struct Trial {
    owner: String,
    reconnect: String,
    since: u64,
    anchor: document::NodeId,
    anchor_move: usize,
    last_activity: Instant,
    disconnected: Option<Instant>,
    messages: VecDeque<Instant>,
}

pub(super) async fn receive(state: &mut State) -> TransportEvent {
    let Some(service) = &mut state.service else {
        return std::future::pending().await;
    };
    match service.events.recv().await {
        Some(event) => event,
        None => std::future::pending().await,
    }
}

pub(super) async fn deadline(deadline: Option<Instant>) {
    match deadline {
        Some(value) => tokio::time::sleep_until(value).await,
        None => std::future::pending().await,
    }
}

impl Actor {
    pub(super) async fn web_board_command(
        &mut self,
        request: Request,
    ) -> Result<WebBoardStatus, AppError> {
        match request {
            Request::Start => self.start_web_service().await?,
            Request::Stop => self.stop_web_board().await?,
            Request::ForceEnd => self.end_web_trial().await?,
            Request::Status => {}
            Request::Options(options) => {
                let mut settings = self.settings.clone();
                settings.web_board = options;
                settings.save(&self.directory).await?;
                self.settings = settings;
                if !self.settings.web_board.allow_trial {
                    self.end_web_trial().await?;
                }
                self.web_hello_all();
            }
        }
        self.refresh_web_board();
        Ok(self.web_status())
    }

    async fn start_web_service(&mut self) -> Result<(), AppError> {
        if self.web_board.service.is_some() {
            return Ok(());
        }
        if self.updates.install_pending {
            return Err(crate::WebBoardError::HostBusy.into());
        }
        let config = self.web_server_config();
        let token = security::random_token()?;
        let (listener, port) = server::bind(&config).await?;
        let mut ips = crate::web_board::addresses::local_addresses().unwrap_or_default();
        if config.bind.is_loopback() || ips.is_empty() {
            ips = vec![Ipv4Addr::LOCALHOST];
        }
        let addresses = ips
            .into_iter()
            .map(|ip| format!("http://{ip}:{port}/{token}/"))
            .collect();
        let (events, receiver) = mpsc::channel(64);
        let (stop, shutdown) = watch::channel(false);
        let assets = self
            .config
            .web_board_assets
            .clone()
            .unwrap_or_else(|| Arc::new(EmptyAssets));
        let task = tokio::spawn(server::run(
            listener,
            token,
            assets,
            events,
            shutdown,
            config.analysis_interval,
        ));
        self.web_board.service = Some(Service {
            port,
            addresses,
            events: receiver,
            stop,
            task,
        });
        self.web_board.stamp = None;
        self.web_board.snapshot = None;
        self.web_board.history = None;
        self.web_board.problems = None;
        self.web_board.trial_view = None;
        Ok(())
    }

    pub(super) async fn stop_web_board(&mut self) -> Result<(), AppError> {
        let trial_result = self.end_web_trial().await;
        if let Some(service) = self.web_board.service.take() {
            let _ = service.stop.send(true);
            // The transport's shutdown cancels event/reply waits before joining sockets.
            let _ = service.task.await;
        }
        self.web_board.clients.clear();
        self.web_board.stamp = None;
        self.web_board.snapshot = None;
        self.publish_web_status();
        trial_result
    }

    pub(super) async fn web_transport_event(&mut self, event: TransportEvent) {
        self.expire_web_trial().await;
        match event {
            TransportEvent::Connected { reconnect, reply } => {
                if self.web_board.service.is_none() {
                    let _ = reply.send(None);
                    return;
                }
                self.refresh_web_board();
                let resumed = self
                    .web_board
                    .trial
                    .as_ref()
                    .filter(|trial| {
                        reconnect
                            .as_ref()
                            .is_some_and(|value| security::token_matches(&trial.reconnect, value))
                    })
                    .map(|trial| (trial.owner.clone(), trial.reconnect.clone()));
                let old_connection = resumed.as_ref().and_then(|(owner, _)| {
                    self.web_board
                        .clients
                        .iter()
                        .find_map(|(connection, client)| {
                            (&client.id == owner).then_some(*connection)
                        })
                });
                if self.web_board.clients.len() >= 20 && old_connection.is_none() {
                    let _ = reply.send(None);
                    return;
                }
                let (id, reconnect) = match resumed {
                    Some(pair) => pair,
                    None => match (security::random_token(), security::random_token()) {
                        (Ok(id), Ok(token)) => (id, token),
                        _ => {
                            let _ = reply.send(None);
                            return;
                        }
                    },
                };
                let Some(connection_id) = self.web_board.sequence.checked_add(1) else {
                    let _ = reply.send(None);
                    return;
                };
                self.web_board.sequence = connection_id;
                // A page reload can register before the old task reports its
                // disconnect. Revoke the old connection atomically and retain
                // the rolling rate window; queued old commands are ignored.
                let messages = old_connection
                    .and_then(|old| self.web_board.clients.remove(&old))
                    .map(|client| client.messages)
                    .unwrap_or_else(|| {
                        self.web_board
                            .trial
                            .as_ref()
                            .filter(|trial| trial.owner == id)
                            .map_or_else(VecDeque::new, |trial| trial.messages.clone())
                    });
                let (outgoing, outbound) = watch::channel(Outbound::default());
                let client = Client {
                    id: id.clone(),
                    reconnect,
                    outgoing,
                    messages,
                };
                self.web_initial(&client);
                self.web_board.clients.insert(connection_id, client);
                if reply
                    .send(Some(ConnectionChannels {
                        connection_id,
                        outbound,
                    }))
                    .is_err()
                {
                    let client = self.web_board.clients.remove(&connection_id);
                    if let Some(trial) = &mut self.web_board.trial
                        && trial.owner == id
                    {
                        trial.disconnected = Some(Instant::now());
                        if let Some(client) = client {
                            trial.messages = client.messages;
                        }
                    }
                } else if let Some(trial) = &mut self.web_board.trial
                    && trial.owner == id
                {
                    trial.disconnected = None;
                }
            }
            TransportEvent::Message {
                connection_id,
                message,
            } => self.web_client_message(connection_id, message).await,
            TransportEvent::Disconnected { connection_id } => {
                if let Some(client) = self.web_board.clients.remove(&connection_id)
                    && let Some(trial) = &mut self.web_board.trial
                    && trial.owner == client.id
                {
                    trial.disconnected = Some(Instant::now());
                    trial.messages = client.messages;
                }
            }
        }
        self.refresh_web_board();
    }

    fn web_server_config(&self) -> server::Config {
        #[cfg(test)]
        if let Some(config) = &self.config.web_board {
            return server::Config {
                bind: config.bind,
                start_port: config.start_port,
                analysis_interval: config.analysis_interval,
            };
        }
        server::Config {
            bind: Ipv4Addr::UNSPECIFIED,
            start_port: 9998,
            analysis_interval: Duration::from_millis(100),
        }
    }

    fn web_status(&self) -> WebBoardStatus {
        let Some(service) = &self.web_board.service else {
            return WebBoardStatus::default();
        };
        WebBoardStatus {
            running: true,
            port: Some(service.port),
            url: service.addresses.first().cloned(),
            addresses: service.addresses.clone(),
            viewers: self.web_board.clients.len(),
            trial: self.web_board.trial.as_ref().map(|trial| WebTrialOwner {
                owner: trial.owner.clone(),
                since: trial.since,
            }),
        }
    }

    fn publish_web_status(&mut self) {
        let status = self.web_status();
        if self.web_board.last_status != status {
            self.web_board.last_status = status.clone();
            self.output.send(AppEvent::WebBoardChanged(status));
        }
    }

    fn web_broadcast(&self, message: WebServerMessage) {
        for client in self.web_board.clients.values() {
            client.send(message.clone());
        }
    }
}

impl Client {
    fn send(&self, message: WebServerMessage) {
        self.outgoing.send_modify(|out| out.push(message));
    }
}
