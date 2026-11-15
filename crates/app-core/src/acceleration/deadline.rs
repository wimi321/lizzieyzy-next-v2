//! A startup deadline can be extended by observed OpenCL tuning, never by silence.
use crate::AccelerationBackend;
use std::time::Duration;
use tokio::time::Instant;

pub(crate) const TUNING_EXTENSION: Duration = Duration::from_secs(120);
pub(crate) const MAX_OPENCL_STARTUP: Duration = Duration::from_secs(1800);

pub(crate) struct StartupDeadline {
    backend: AccelerationBackend,
    deadline: Instant,
    maximum: Instant,
}

impl StartupDeadline {
    pub(crate) fn new(backend: AccelerationBackend, now: Instant) -> Self {
        let timeout = match backend {
            AccelerationBackend::Cuda => Duration::from_secs(180),
            AccelerationBackend::TensorRt | AccelerationBackend::OpenCl => Duration::from_secs(600),
        };
        Self::with_timeout(backend, now, timeout)
    }

    pub(crate) fn with_timeout(
        backend: AccelerationBackend,
        now: Instant,
        timeout: Duration,
    ) -> Self {
        let maximum = if backend == AccelerationBackend::OpenCl {
            now + MAX_OPENCL_STARTUP
        } else {
            now + timeout
        };
        Self {
            backend,
            deadline: (now + timeout).min(maximum),
            maximum,
        }
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }

    /// A late line cannot revive an already expired attempt.
    pub(crate) fn on_tuning(&mut self, now: Instant) -> bool {
        if self.backend != AccelerationBackend::OpenCl || now >= self.deadline {
            return false;
        }
        self.deadline = (self.deadline + TUNING_EXTENSION).min(self.maximum);
        true
    }
}

#[cfg(test)]
mod tests;
