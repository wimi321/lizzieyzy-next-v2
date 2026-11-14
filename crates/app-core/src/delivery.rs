use crate::AppEvent;
use tokio::sync::watch;

// Latest-state slots keep delivery bounded without ever blocking edits.
// Clearing analysis on a transition invalidates even an unread old result.
#[derive(Clone, Default)]
struct Mailbox {
    serial: u64,
    document: Option<(u64, AppEvent)>,
    analysis: Option<(u64, AppEvent)>,
    engine: Option<(u64, AppEvent)>,
    review: Option<(u64, AppEvent)>,
    batch: Option<(u64, AppEvent)>,
    game: Option<(u64, AppEvent)>,
    match_game: Option<(u64, AppEvent)>,
    console: std::collections::VecDeque<(u64, AppEvent)>,
    board_sync: Option<(u64, AppEvent)>,
    benchmark: Option<(u64, AppEvent)>,
    analysis_state: Option<(u64, AppEvent)>,
    open_failed: Option<(u64, AppEvent)>,
    asset_download: Option<(u64, AppEvent)>,
    update_download: Option<(u64, AppEvent)>,
}

pub(crate) struct Delivery(watch::Sender<Mailbox>);

pub struct AppSubscription {
    receiver: watch::Receiver<Mailbox>,
    read: [Option<u64>; 13],
    console_read: u64,
}

pub(crate) fn channel() -> (Delivery, AppSubscription) {
    let (sender, receiver) = watch::channel(Mailbox::default());
    (
        Delivery(sender),
        AppSubscription {
            receiver,
            read: [None; 13],
            console_read: 0,
        },
    )
}

impl Delivery {
    pub fn send(&self, event: AppEvent) {
        self.0.send_if_modified(|mailbox| {
            if matches!(event, AppEvent::AnalysisStateChanged(_))
                && mailbox
                    .analysis_state
                    .as_ref()
                    .is_some_and(|(_, previous)| previous == &event)
            {
                return false;
            }
            mailbox.serial = mailbox.serial.wrapping_add(1);
            if matches!(event, AppEvent::GtpConsoleLine(_)) {
                mailbox.console.push_back((mailbox.serial, event));
                if mailbox.console.len() > 2000 {
                    mailbox.console.pop_front();
                }
                return true;
            }
            let slot = match &event {
                AppEvent::GtpConsoleLine(_) => return false,
                AppEvent::MatchUpdated(_) => &mut mailbox.match_game,
                AppEvent::UpdateDownloadProgress(_) => &mut mailbox.update_download,
                AppEvent::AssetDownloadProgress(_) => &mut mailbox.asset_download,
                AppEvent::DocumentChanged { .. } => &mut mailbox.document,
                AppEvent::AnalysisUpdated(_) => &mut mailbox.analysis,
                AppEvent::EngineStatusChanged(_) => &mut mailbox.engine,
                AppEvent::WholeGameProgress(_) => &mut mailbox.review,
                AppEvent::BatchAnalysisProgress(_) => &mut mailbox.batch,
                AppEvent::GameUpdated(_) => &mut mailbox.game,
                AppEvent::BoardSyncStatusChanged(_) => &mut mailbox.board_sync,
                AppEvent::BenchmarkProgress(_) => &mut mailbox.benchmark,
                AppEvent::AnalysisStateChanged(_) => &mut mailbox.analysis_state,
                AppEvent::OpenFailed { .. } => &mut mailbox.open_failed,
            };
            *slot = Some((mailbox.serial, event));
            true
        });
    }

    pub fn clear_analysis(&self) {
        self.0.send_modify(|mailbox| mailbox.analysis = None);
    }

    /// A coalesced document can otherwise overtake its unread focus result.
    /// Replay the original frame after it, preserving Live/Cache/Saved and speed.
    pub fn republish_analysis(&self, revision: u64, node_id: u32) {
        self.0.send_if_modified(|mailbox| {
            let Some((serial, AppEvent::AnalysisUpdated(view))) = &mut mailbox.analysis else {
                return false;
            };
            if (view.revision, view.node_id) != (revision, node_id) {
                return false;
            }
            mailbox.serial = mailbox.serial.wrapping_add(1);
            *serial = mailbox.serial;
            true
        });
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
                    &mailbox.board_sync,
                    &mailbox.benchmark,
                    &mailbox.analysis_state,
                    &mailbox.open_failed,
                    &mailbox.asset_download,
                    &mailbox.update_download,
                    &mailbox.batch,
                    &mailbox.game,
                    &mailbox.match_game,
                ]
                .into_iter()
                .enumerate()
                .filter_map(|(i, slot)| slot.as_ref().map(|(serial, event)| (i, *serial, event)))
                .filter(|(i, serial, _)| self.read[*i] != Some(*serial))
                .min_by_key(|(_, serial, _)| *serial);
                if let Some((serial, event)) = mailbox
                    .console
                    .iter()
                    .find(|(serial, _)| *serial > self.console_read)
                    && next
                        .as_ref()
                        .is_none_or(|(_, next_serial, _)| serial < next_serial)
                {
                    self.console_read = *serial;
                    return Some(event.clone());
                }
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
