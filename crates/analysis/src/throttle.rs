use std::time::Duration;

use document::Revision;
use tokio::time::Instant;

use crate::AnalysisSnapshot;

const INTERVAL: Duration = Duration::from_millis(50);

/// Latest-value throttle with a trailing delivery, including the final result.
/// The actor clears pending data whenever its query is cancelled or restarted.
pub(crate) struct Throttle {
    next_allowed: Option<Instant>,
    pending: Option<AnalysisSnapshot>,
}

impl Throttle {
    pub(crate) fn new() -> Self {
        Self {
            next_allowed: None,
            pending: None,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.next_allowed = None;
        self.pending = None;
    }

    pub(crate) fn clear_pending(&mut self) {
        self.pending = None;
    }

    /// Retag a continuing query without discarding a buffered final. An
    /// immediate identity refresh starts a fresh interval for later live data.
    pub(crate) fn rebase_revision(&mut self, revision: Revision, now: Instant, republished: bool) {
        if let Some(snapshot) = &mut self.pending {
            snapshot.revision = revision;
        }
        if republished {
            self.next_allowed = Some(now + INTERVAL);
        }
    }

    pub(crate) fn push(
        &mut self,
        snapshot: AnalysisSnapshot,
        now: Instant,
    ) -> Option<AnalysisSnapshot> {
        self.pending = Some(snapshot);
        self.flush(now)
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.pending.as_ref().and(self.next_allowed)
    }

    pub(crate) fn flush(&mut self, now: Instant) -> Option<AnalysisSnapshot> {
        if self.next_allowed.is_some_and(|deadline| now < deadline) {
            return None;
        }
        let snapshot = self.pending.take()?;
        // Schedule from actual delivery time: a busy runtime never catches up
        // with a burst of missed ticks.
        self.next_allowed = Some(now + INTERVAL);
        Some(snapshot)
    }
}

#[cfg(test)]
mod tests;
