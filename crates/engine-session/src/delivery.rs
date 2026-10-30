use crate::{QueryEvent, QueryId};
use tokio::sync::watch;

/// A single watch slot. Reliable history has a fixed maximum of 256 finals,
/// 32 restarts and one terminal failure; only ONE partial is retained.
#[derive(Clone, Debug, Default)]
struct Mailbox {
    serial: u64,
    reliable: Vec<(u64, QueryEvent)>,
    partial: Option<(u64, QueryEvent)>,
    closed: bool,
}

pub(crate) struct Delivery(watch::Sender<Mailbox>);

pub struct QuerySubscription {
    id: QueryId,
    receiver: watch::Receiver<Mailbox>,
    reliable_read: usize,
    partial_read: u64,
}

pub(crate) fn channel(id: QueryId) -> (Delivery, QuerySubscription) {
    let (sender, receiver) = watch::channel(Mailbox::default());
    (
        Delivery(sender),
        QuerySubscription {
            id,
            receiver,
            reliable_read: 0,
            partial_read: 0,
        },
    )
}

impl Delivery {
    pub fn send(&self, event: QueryEvent, closed: bool) {
        self.0.send_modify(|mailbox| {
            mailbox.serial += 1;
            if matches!(event, QueryEvent::Partial(_)) {
                mailbox.partial = Some((mailbox.serial, event));
            } else {
                if matches!(
                    event,
                    QueryEvent::Restarted | QueryEvent::Terminated | QueryEvent::Failed(_)
                ) {
                    mailbox.partial = None;
                }
                mailbox.reliable.push((mailbox.serial, event));
            }
            mailbox.closed = closed;
        });
    }
}

impl QuerySubscription {
    /// Stable cancellation identity; response ids change after process restart.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Cancellation safe. Slow readers see only the latest unconsumed Partial;
    /// reliable events remain available even after the session has shut down.
    pub async fn recv(&mut self) -> Option<QueryEvent> {
        loop {
            {
                let mailbox = self.receiver.borrow_and_update();
                let reliable = mailbox.reliable.get(self.reliable_read);
                let partial = mailbox
                    .partial
                    .as_ref()
                    .filter(|(serial, _)| *serial > self.partial_read);
                if let Some((serial, event)) = partial
                    && reliable.is_none_or(|(next, _)| serial < next)
                {
                    self.partial_read = *serial;
                    return Some(event.clone());
                }
                if let Some((_, event)) = reliable {
                    self.reliable_read += 1;
                    return Some(event.clone());
                }
                if mailbox.closed {
                    return None;
                }
            }
            if self.receiver.changed().await.is_err() {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests;
