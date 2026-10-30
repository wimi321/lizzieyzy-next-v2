use crate::{AnalysisSnapshot, SnapshotSource};
use tokio::sync::watch;

/// One cache result followed by the latest live result, bounded independently
/// of consumer speed. Focus invalidation clears both, including unread data.
#[derive(Clone, Default)]
struct Mailbox {
    serial: u64,
    cached: Option<(u64, AnalysisSnapshot)>,
    live: Option<(u64, AnalysisSnapshot)>,
}

pub(crate) struct Delivery(watch::Sender<Mailbox>);

pub struct AnalysisSubscription {
    receiver: watch::Receiver<Mailbox>,
    cache_read: Option<u64>,
    live_read: Option<u64>,
}

pub(crate) fn channel() -> (Delivery, AnalysisSubscription) {
    let (sender, receiver) = watch::channel(Mailbox::default());
    (
        Delivery(sender),
        AnalysisSubscription {
            receiver,
            cache_read: None,
            live_read: None,
        },
    )
}

impl Delivery {
    pub fn send(&self, snapshot: AnalysisSnapshot) {
        self.0.send_modify(|mailbox| {
            mailbox.serial = mailbox.serial.wrapping_add(1);
            let slot = match snapshot.source {
                SnapshotSource::Cache => &mut mailbox.cached,
                SnapshotSource::Live => &mut mailbox.live,
            };
            *slot = Some((mailbox.serial, snapshot));
        });
    }

    pub fn clear(&self) {
        self.0.send_modify(|mailbox| {
            mailbox.cached = None;
            mailbox.live = None;
        });
    }
}

impl AnalysisSubscription {
    /// Cancellation safe. Unread cache precedes live data; intermediate live
    /// snapshots coalesce, while the final remains until a command invalidates it.
    pub async fn recv(&mut self) -> Option<AnalysisSnapshot> {
        loop {
            {
                let mailbox = self.receiver.borrow_and_update();
                if let Some((serial, snapshot)) = &mailbox.cached
                    && self.cache_read != Some(*serial)
                {
                    self.cache_read = Some(*serial);
                    return Some(snapshot.clone());
                }
                if let Some((serial, snapshot)) = &mailbox.live
                    && self.live_read != Some(*serial)
                {
                    self.live_read = Some(*serial);
                    return Some(snapshot.clone());
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
