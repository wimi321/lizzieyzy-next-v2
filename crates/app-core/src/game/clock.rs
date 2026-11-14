use super::{HumanClock, HumanClockView};
use std::time::Duration;
use tokio::time::Instant;

/// Owned by the app actor. No timer or wall clock is hidden inside this value.
#[derive(Clone, Debug)]
pub(crate) struct ClockState {
    main: Duration,
    period: Duration,
    remaining: Duration,
    periods: u32,
    started: Option<Instant>,
}

impl ClockState {
    pub(crate) fn new(settings: HumanClock) -> Self {
        let period = Duration::from_secs(u64::from(settings.byoyomi_seconds));
        Self {
            main: Duration::from_secs(u64::from(settings.main_seconds)),
            period,
            remaining: period,
            periods: settings.periods,
            started: None,
        }
    }

    pub(crate) fn start(&mut self, now: Instant) {
        if self.started.is_none() {
            self.started = Some(now);
        }
    }

    pub(crate) fn pause(&mut self, now: Instant) -> bool {
        let expired = self.update(now);
        self.started = None;
        expired
    }

    /// Consumes elapsed human time, retaining partially used periods on pause.
    pub(crate) fn update(&mut self, now: Instant) -> bool {
        if let Some(started) = self.started {
            self.started = Some(now);
            let mut elapsed = now.saturating_duration_since(started);
            let main_used = elapsed.min(self.main);
            self.main -= main_used;
            elapsed -= main_used;
            if self.main.is_zero() {
                if self.period.is_zero() || self.periods == 0 {
                    self.periods = 0;
                    self.remaining = Duration::ZERO;
                } else if elapsed >= self.remaining {
                    elapsed -= self.remaining;
                    self.periods -= 1;
                    // Integer-duration arithmetic preserves the exact timeout boundary.
                    let consumed = elapsed.as_nanos() / self.period.as_nanos();
                    if consumed >= u128::from(self.periods) {
                        self.periods = 0;
                        self.remaining = Duration::ZERO;
                    } else {
                        self.periods -= consumed as u32;
                        let remainder = elapsed.as_nanos() % self.period.as_nanos();
                        let used = Duration::new(
                            (remainder / 1_000_000_000) as u64,
                            (remainder % 1_000_000_000) as u32,
                        );
                        self.remaining = self.period - used;
                    }
                } else {
                    self.remaining -= elapsed;
                }
            }
        }
        self.main.is_zero() && self.periods == 0
    }

    pub(crate) fn finish_turn(&mut self, now: Instant) -> bool {
        let expired = self.pause(now);
        if !expired {
            self.remaining = self.period;
        }
        expired
    }

    pub(crate) fn view(&self, now: Instant) -> HumanClockView {
        let mut current = self.clone();
        current.update(now);
        HumanClockView {
            main_seconds: current.main.as_secs_f64(),
            periods: current.periods,
            byoyomi_seconds: current.remaining.as_secs_f64(),
        }
    }

    /// Next main-time / period boundary, not a safety-net timeout.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.started.map(|now| {
            now + if self.main.is_zero() {
                self.remaining
            } else {
                self.main
            }
        })
    }
}
