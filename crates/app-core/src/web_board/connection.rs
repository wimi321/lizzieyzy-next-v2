use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, timeout};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{Message, protocol::frame::coding::CloseCode},
};

use super::protocol::{WebClientMessage, WebServerMessage};

const KINDS: usize = 8;
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) enum TransportEvent {
    Connected {
        reconnect: Option<String>,
        reply: oneshot::Sender<Option<ConnectionChannels>>,
    },
    Message {
        connection_id: u64,
        message: WebClientMessage,
    },
    Disconnected {
        connection_id: u64,
    },
}

pub(crate) struct ConnectionChannels {
    pub connection_id: u64,
    pub outbound: watch::Receiver<Outbound>,
}

#[derive(Clone, Default)]
pub(crate) struct Outbound {
    sequence: u64,
    slots: [Option<(u64, WebServerMessage)>; KINDS],
}

impl Outbound {
    pub(crate) fn push(&mut self, message: WebServerMessage) {
        let kind = message_kind(&message);
        if kind == 1 {
            self.slots[2] = None;
        }
        self.sequence = self.sequence.saturating_add(1);
        self.slots[kind] = Some((self.sequence, message));
    }

    fn next(
        &self,
        seen: &[u64; KINDS],
        analysis_ready: bool,
    ) -> Option<(usize, u64, WebServerMessage)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(kind, entry)| {
                let (sequence, message) = entry.as_ref()?;
                (*sequence > seen[kind] && (kind != 2 || analysis_ready))
                    .then_some((kind, *sequence, message))
            })
            // Identity always precedes a new viewer's state; board changes take
            // priority over ancillary snapshots and throttled analysis.
            .min_by_key(|(kind, sequence, _)| ((*kind).min(2), *sequence))
            .map(|(kind, sequence, message)| (kind, sequence, message.clone()))
    }

    fn pending_analysis(&self, seen: &[u64; KINDS]) -> bool {
        self.slots[2]
            .as_ref()
            .is_some_and(|(sequence, _)| *sequence > seen[2])
    }
}

fn message_kind(message: &WebServerMessage) -> usize {
    match message {
        WebServerMessage::Hello(_) => 0,
        WebServerMessage::FullState(_) => 1,
        WebServerMessage::AnalysisUpdate(_) => 2,
        WebServerMessage::WinrateHistory(_) => 3,
        WebServerMessage::ProblemMoves(_) => 4,
        WebServerMessage::TrialState(_) => 5,
        WebServerMessage::TrialDenied { .. } => 6,
        WebServerMessage::TrialRejected { .. } => 7,
    }
}

type Socket = WebSocketStream<TokioIo<Upgraded>>;

pub(super) async fn run(
    mut socket: Socket,
    reconnect: Option<String>,
    events: mpsc::Sender<TransportEvent>,
    mut stop: watch::Receiver<bool>,
    analysis_interval: Duration,
) {
    let (reply, response) = oneshot::channel();
    let registration = async {
        events
            .send(TransportEvent::Connected { reconnect, reply })
            .await
            .ok()?;
        response.await.ok().flatten()
    };
    let channels = tokio::select! {
        biased;
        _ = stopped(&mut stop) => return,
        channels = registration => channels,
    };
    let Some(channels) = channels else {
        close(&mut socket, CloseCode::Again).await;
        return;
    };
    let id = channels.connection_id;
    drive(&mut socket, channels, &events, &mut stop, analysis_interval).await;
    close(&mut socket, CloseCode::Away).await;
    tokio::select! {
        biased;
        _ = stopped(&mut stop) => {},
        _ = events.send(TransportEvent::Disconnected { connection_id: id }) => {},
    }
}

async fn drive(
    socket: &mut Socket,
    mut channels: ConnectionChannels,
    events: &mpsc::Sender<TransportEvent>,
    stop: &mut watch::Receiver<bool>,
    analysis_interval: Duration,
) {
    let mut seen = [0; KINDS];
    let mut next_analysis = Instant::now();
    loop {
        if *stop.borrow() {
            break;
        }
        // Never queue into tungstenite while its previous send is unflushed.
        // After each flush, read the current watch slots, discarding intermediate
        // updates even when the socket has been backpressured for a long time.
        let outbound = channels.outbound.borrow_and_update().clone();
        if let Some((kind, sequence, message)) =
            outbound.next(&seen, Instant::now() >= next_analysis)
        {
            let Ok(text) = serde_json::to_string(&message) else {
                break;
            };
            let sent = tokio::select! {
                biased;
                _ = stopped(stop) => false,
                result = timeout(WRITE_TIMEOUT, socket.send(Message::Text(text.into()))) => {
                    matches!(result, Ok(Ok(())))
                }
            };
            if !sent {
                break;
            }
            seen[kind] = sequence;
            if kind == 2 {
                next_analysis = Instant::now() + analysis_interval;
            }
            continue;
        }
        let pending_analysis = outbound.pending_analysis(&seen);
        tokio::select! {
            biased;
            _ = stopped(stop) => break,
            changed = channels.outbound.changed() => {
                if changed.is_err() { break; }
            },
            _ = tokio::time::sleep_until(next_analysis), if pending_analysis => {},
            frame = socket.next() => {
                let frame = match frame {
                    Some(Ok(frame)) => frame,
                    Some(Err(tokio_tungstenite::tungstenite::Error::Capacity(_))) => {
                        close(socket, CloseCode::Size).await;
                        break;
                    },
                    _ => break,
                };
                match frame {
                    Message::Text(text) => {
                        let Ok(message) = serde_json::from_str(&text) else {
                            // Malformed commands never reach the document actor.
                            let rejected = WebServerMessage::TrialRejected {
                                reason: "invalid message".into(),
                            };
                            let Ok(text) = serde_json::to_string(&rejected) else { break; };
                            let result = timeout(WRITE_TIMEOUT, socket.send(Message::Text(text.into()))).await;
                            if !matches!(result, Ok(Ok(()))) { break; }
                            continue;
                        };
                        tokio::select! {
                            biased;
                            _ = stopped(stop) => break,
                            result = events.send(TransportEvent::Message {
                                connection_id: channels.connection_id, message,
                            }) => { if result.is_err() { break; } },
                        }
                    },
                    Message::Ping(_) => {
                        if !matches!(timeout(WRITE_TIMEOUT, socket.flush()).await, Ok(Ok(()))) {
                            break;
                        }
                    },
                    Message::Pong(_) => {},
                    Message::Close(_) => break,
                    Message::Binary(_) | Message::Frame(_) => {
                        close(socket, CloseCode::Unsupported).await;
                        break;
                    },
                }
            },
        }
    }
}

pub(super) async fn stopped(stop: &mut watch::Receiver<bool>) {
    loop {
        if *stop.borrow_and_update() || stop.changed().await.is_err() {
            return;
        }
    }
}

async fn close(socket: &mut Socket, code: CloseCode) {
    let frame = tokio_tungstenite::tungstenite::protocol::CloseFrame {
        code,
        reason: "".into(),
    };
    let _ = timeout(WRITE_TIMEOUT, socket.close(Some(frame))).await;
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
