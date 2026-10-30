//! One consumer and one Channel preserve AppCore's cross-event ordering.
use app_core::{AppEvent, AppSubscription};
use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot};

use crate::error::CommandError;

enum Request {
    Subscribe(
        Channel<AppEvent>,
        oneshot::Sender<Result<u32, CommandError>>,
    ),
    Unsubscribe(u32, oneshot::Sender<()>),
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

    pub async fn subscribe(&self, channel: Channel<AppEvent>) -> Result<u32, CommandError> {
        let (reply, response) = oneshot::channel();
        self.requests
            .send(Request::Subscribe(channel, reply))
            .await
            .map_err(|_| CommandError::StreamClosed)?;
        response.await.map_err(|_| CommandError::StreamClosed)?
    }

    pub async fn unsubscribe(&self, id: u32) -> Result<(), CommandError> {
        let (reply, response) = oneshot::channel();
        self.requests
            .send(Request::Unsubscribe(id, reply))
            .await
            .map_err(|_| CommandError::StreamClosed)?;
        response.await.map_err(|_| CommandError::StreamClosed)
    }
}

async fn forward(mut subscription: AppSubscription, mut requests: mpsc::Receiver<Request>) {
    let mut active: Option<(u32, Channel<AppEvent>)> = None;
    let mut next_id = Some(1_u32);
    // At most three state events (document, engine, review progress), retained
    // in their original publication order.
    // Analysis is deliberately never replayed: pausing can invalidate it without
    // a document change. Only AppCore decides which new analysis may be sent.
    let mut replay = Vec::<AppEvent>::new();
    loop {
        tokio::select! {
            biased;
            request = requests.recv() => match request {
                Some(Request::Subscribe(channel, reply)) => {
                    let Some(id) = next_id else {
                        let _ = reply.send(Err(CommandError::SubscriptionLimit));
                        continue;
                    };
                    next_id = id.checked_add(1);
                    active = None;
                    let result = replay.iter().try_for_each(|event| channel.send(event.clone()));
                    match result {
                        Ok(()) => {
                            active = Some((id, channel));
                            let _ = reply.send(Ok(id));
                        }
                        Err(error) => {
                            let _ = reply.send(Err(CommandError::Channel(error.to_string())));
                        }
                    }
                }
                Some(Request::Unsubscribe(id, reply)) => {
                    if active.as_ref().is_some_and(|(current, _)| *current == id) {
                        active = None;
                    }
                    let _ = reply.send(());
                }
                None => break,
            },
            // Leave unread events in AppCore's bounded, invalidation-aware
            // mailbox while disconnected, including a completed final result.
            event = subscription.recv(), if active.is_some() => {
                let Some(event) = event else { break };
                if !matches!(event, AppEvent::AnalysisUpdated(_)) {
                    replay.retain(|cached| std::mem::discriminant(cached) != std::mem::discriminant(&event));
                    replay.push(event.clone());
                }
                if let Some((_, channel)) = &active
                    && channel.send(event).is_err()
                {
                    active = None;
                }
            }
        }
    }
}
