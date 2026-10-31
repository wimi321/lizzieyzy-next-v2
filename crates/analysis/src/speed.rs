use std::{collections::VecDeque, time::Duration};
use tokio::time::Instant;

const WINDOW: Duration = Duration::from_secs(3);
const MIN_INTERVAL: Duration = Duration::from_millis(500);

/// Owned by one live query. Final results and idle time do not add samples.
#[derive(Default)]
pub(crate) struct SearchSpeed {
    samples: VecDeque<(Instant, u64)>,
    visits_per_second: Option<f64>,
}

impl SearchSpeed {
    pub fn record(&mut self, visits: u64) {
        let now = Instant::now();
        // A decreasing counter cannot yield a meaningful rate. Start a fresh
        // interval defensively even if no restart notification preceded it.
        if self.samples.back().is_some_and(|(_, last)| visits < *last) {
            self.samples.clear();
        }
        while self
            .samples
            .front()
            .is_some_and(|(at, _)| now.duration_since(*at) > WINDOW)
        {
            self.samples.pop_front();
        }
        self.samples.push_back((now, visits));
        self.visits_per_second = self.samples.front().and_then(|(first_at, first_visits)| {
            let elapsed = now.duration_since(*first_at);
            (elapsed >= MIN_INTERVAL)
                .then(|| (visits - first_visits) as f64 / elapsed.as_secs_f64())
        });
    }

    pub fn visits_per_second(&self) -> Option<f64> {
        self.visits_per_second
    }
}

#[cfg(test)]
mod tests;
