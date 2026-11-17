//! One consumer broadcasts ordered AppCore events to one Channel per webview.
use std::collections::BTreeMap;

use app_core::{AppEvent, AppSubscription};
use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot};

use crate::error::CommandError;

enum Request {
    Subscribe(
        String,
        Channel<AppEvent>,
        oneshot::Sender<Result<u32, CommandError>>,
    ),
    Unsubscribe(String, u32, oneshot::Sender<()>),
}

#[derive(Clone)]
pub struct EventBridge {
    requests: mpsc::Sender<Request>,
}

impl EventBridge {
    pub fn new(subscription: AppSubscription) -> Self {
        let (requests, receiver) = mpsc::channel(64);
        tauri::async_runtime::spawn(forward(subscription, receiver));
        Self { requests }
    }

    pub async fn subscribe(
        &self,
        owner: String,
        channel: Channel<AppEvent>,
    ) -> Result<u32, CommandError> {
        let (reply, response) = oneshot::channel();
        self.requests
            .send(Request::Subscribe(owner, channel, reply))
            .await
            .map_err(|_| CommandError::StreamClosed)?;
        response.await.map_err(|_| CommandError::StreamClosed)?
    }

    pub async fn unsubscribe(&self, owner: String, id: u32) -> Result<(), CommandError> {
        let (reply, response) = oneshot::channel();
        self.requests
            .send(Request::Unsubscribe(owner, id, reply))
            .await
            .map_err(|_| CommandError::StreamClosed)?;
        response.await.map_err(|_| CommandError::StreamClosed)
    }
}

async fn forward(mut subscription: AppSubscription, mut requests: mpsc::Receiver<Request>) {
    let mut active = BTreeMap::<String, (u32, Channel<AppEvent>)>::new();
    let mut next_id = Some(1_u32);
    // Keep one latest event per state domain, including coaching state/summary,
    // retained in their original publication order. Individual move ratings
    // are transient; CoachView contains the current rating for reconnection.
    // Analysis is deliberately never replayed: pausing can invalidate it without
    // a document change. Only AppCore decides which new analysis may be sent.
    let mut replay = Vec::<AppEvent>::new();
    loop {
        tokio::select! {
            biased;
            request = requests.recv() => match request {
                Some(Request::Subscribe(owner, channel, reply)) => {
                    let Some(id) = next_id else {
                        let _ = reply.send(Err(CommandError::SubscriptionLimit));
                        continue;
                    };
                    next_id = id.checked_add(1);
                    active.remove(&owner);
                    let result = replay.iter().try_for_each(|event| channel.send(event.clone()));
                    match result {
                        Ok(()) => {
                            active.insert(owner, (id, channel));
                            let _ = reply.send(Ok(id));
                        }
                        Err(error) => {
                            let _ = reply.send(Err(CommandError::Channel(error.to_string())));
                        }
                    }
                }
                Some(Request::Unsubscribe(owner, id, reply)) => {
                    if active.get(&owner).is_some_and(|(current, _)| *current == id) {
                        active.remove(&owner);
                    }
                    let _ = reply.send(());
                }
                None => break,
            },
            // Leave unread events in AppCore's bounded, invalidation-aware
            // mailbox while disconnected, including a completed final result.
            event = subscription.recv(), if !active.is_empty() => {
                let Some(event) = event else { break };
                remember_state(&mut replay, &event);
                active.retain(|_, (_, channel)| channel.send(event.clone()).is_ok());
            }
        }
    }
}

fn remember_state(replay: &mut Vec<AppEvent>, event: &AppEvent) {
    if matches!(
        event,
        AppEvent::AnalysisUpdated(_)
            | AppEvent::ComparisonAnalysisUpdated(_)
            | AppEvent::CoachMoveRated(_)
            | AppEvent::GtpConsoleLine(_)
            | AppEvent::OpenFailed { .. }
            | AppEvent::CaptureEnded { .. }
    ) {
        return;
    }
    // A new coaching session must not replay the previous game's summary.
    if matches!(event, AppEvent::CoachUpdated(view) if view.summary.is_none()) {
        replay.retain(|cached| !matches!(cached, AppEvent::CoachSummary(_)));
    }
    replay.retain(|cached| std::mem::discriminant(cached) != std::mem::discriminant(event));
    let mut cached = event.clone();
    if let AppEvent::ComparisonChanged(view) = &mut cached {
        // The status is durable; its embedded analysis has the same transient
        // validity as ComparisonAnalysisUpdated, including same-revision changes.
        view.analysis = None;
    }
    replay.push(cached);
}

#[cfg(test)]
mod tests;
