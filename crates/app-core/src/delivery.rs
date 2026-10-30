use crate::AppEvent;
use tokio::sync::watch;

// Four latest-state slots keep delivery bounded without ever blocking edits.
// Clearing analysis on a transition invalidates even an unread old result.
#[derive(Clone, Default)]
struct Mailbox {
    serial: u64,
    document: Option<(u64, AppEvent)>,
    analysis: Option<(u64, AppEvent)>,
    engine: Option<(u64, AppEvent)>,
    review: Option<(u64, AppEvent)>,
}

pub(crate) struct Delivery(watch::Sender<Mailbox>);

pub struct AppSubscription {
    receiver: watch::Receiver<Mailbox>,
    read: [Option<u64>; 4],
}

pub(crate) fn channel() -> (Delivery, AppSubscription) {
    let (sender, receiver) = watch::channel(Mailbox::default());
    (
        Delivery(sender),
        AppSubscription {
            receiver,
            read: [None; 4],
        },
    )
}

impl Delivery {
    pub fn send(&self, event: AppEvent) {
        self.0.send_modify(|mailbox| {
            mailbox.serial = mailbox.serial.wrapping_add(1);
            let slot = match &event {
                AppEvent::DocumentChanged { .. } => &mut mailbox.document,
                AppEvent::AnalysisUpdated(_) => &mut mailbox.analysis,
                AppEvent::EngineStatusChanged(_) => &mut mailbox.engine,
                AppEvent::WholeGameProgress(_) => &mut mailbox.review,
            };
            *slot = Some((mailbox.serial, event));
        });
    }

    pub fn clear_analysis(&self) {
        self.0.send_modify(|mailbox| mailbox.analysis = None);
    }
}

impl AppSubscription {
    /// Cancellation safe; document/status updates coalesce and analysis holds
    /// only the latest result. Remaining events are delivered in publish order.
    /// After a navigation command succeeds, no unread old analysis survives.
    pub async fn recv(&mut self) -> Option<AppEvent> {
        loop {
            {
                let mailbox = self.receiver.borrow_and_update();
                let next = [
                    &mailbox.document,
                    &mailbox.analysis,
                    &mailbox.engine,
                    &mailbox.review,
                ]
                .into_iter()
                .enumerate()
                .filter_map(|(i, slot)| slot.as_ref().map(|(serial, event)| (i, *serial, event)))
                .filter(|(i, serial, _)| self.read[*i] != Some(*serial))
                .min_by_key(|(_, serial, _)| *serial);
                if let Some((i, serial, event)) = next {
                    self.read[i] = Some(serial);
                    return Some(event.clone());
                }
            }
            if self.receiver.changed().await.is_err() {
                return None;
            }
        }
    }
}
