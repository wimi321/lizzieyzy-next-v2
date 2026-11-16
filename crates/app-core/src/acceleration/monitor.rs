//! Session-bound, bounded stderr observation with throttled UI progress.
use super::output::{OutputTail, reported_backend};
use engine_session::StartupDeadline;
use std::{future::pending, time::Duration};
use tokio::{sync::broadcast, time::Instant};

const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

pub(crate) struct Monitor {
    receiver: broadcast::Receiver<String>,
    pub clock: StartupDeadline,
    tail: OutputTail,
    pub backend_line: Option<String>,
    next_progress: Instant,
    tuning_pending: bool,
    startup: bool,
    timeout: Duration,
    maximum: Duration,
    restart_attempt: u32,
}

impl Monitor {
    pub(crate) fn new(
        receiver: broadcast::Receiver<String>,
        started: Instant,
        timeout: Duration,
        maximum: Duration,
    ) -> Self {
        Self {
            receiver,
            clock: StartupDeadline::new(started, timeout, maximum),
            tail: OutputTail::default(),
            backend_line: None,
            next_progress: started,
            tuning_pending: false,
            startup: true,
            timeout,
            maximum,
            restart_attempt: 0,
        }
    }

    fn line(&mut self, line: &str, now: Instant) {
        self.tail.push(line);
        if reported_backend(line).is_some() {
            // Preserve the early backend identity independently of the tail.
            self.backend_line = Some(line.into());
        }
        if self.startup && self.clock.observe(line, now) {
            self.tuning_pending = true;
        }
    }

    /// Drain at most a broadcast window so noisy stderr cannot starve commands.
    pub(crate) fn drain(&mut self) {
        let mut lines = 0;
        // Lag reports do not consume a line's budget; a second bound prevents
        // an endlessly overflowing producer from keeping this actor turn alive.
        for _ in 0..512 {
            match self.receiver.try_recv() {
                Ok(line) => {
                    self.line(&line, Instant::now());
                    lines += 1;
                    if lines == 256 {
                        break;
                    }
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    }

    pub(crate) fn lines(&self) -> Vec<String> {
        self.tail.lines()
    }

    pub(crate) fn restarted(&mut self, attempt: u32, now: Instant) {
        if attempt > self.restart_attempt {
            self.restart_attempt = attempt;
            self.clock = StartupDeadline::new(now, self.timeout, self.maximum);
            self.tuning_pending = false;
        }
    }

    pub(crate) fn finish(&mut self) {
        self.startup = false;
        self.tuning_pending = false;
    }

    pub(crate) fn progress(&mut self, now: Instant) -> bool {
        if !self.tuning_pending || now < self.next_progress {
            return false;
        }
        self.tuning_pending = false;
        self.next_progress = now + PROGRESS_INTERVAL;
        true
    }

    /// Reading never blocks the engine: lag drops old lines, keeping the latest.
    pub(crate) async fn next(&mut self) -> bool {
        loop {
            tokio::select! {
                line = self.receiver.recv() => match line {
                    Ok(line) => {
                        self.line(&line, Instant::now());
                        self.drain();
                        return self.progress(Instant::now());
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return pending().await,
                },
                _ = tokio::time::sleep_until(self.next_progress), if self.tuning_pending => {
                    return self.progress(Instant::now());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
