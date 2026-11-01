use std::time::Duration;

use tokio::time::Instant;

use crate::AnalysisError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitReason {
    Time,
    Playouts,
}

/// Limits apply only to the foreground coordinator, never review queries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisLimits {
    pub limit_time: bool,
    pub max_analyze_time_seconds: u32,
    pub limit_playout: bool,
    pub limit_playouts: u64,
}

impl Default for AnalysisLimits {
    fn default() -> Self {
        Self {
            limit_time: true,
            max_analyze_time_seconds: 600,
            limit_playout: false,
            limit_playouts: 100_000,
        }
    }
}

impl AnalysisLimits {
    pub fn validate(&self) -> Result<(), AnalysisError> {
        if self.max_analyze_time_seconds == 0 || self.limit_playouts == 0 {
            return Err(AnalysisError::InvalidSettings(
                "analysis time and playout limits must be positive",
            ));
        }
        Ok(())
    }

    pub(crate) fn deadline(&self, started: Option<Instant>) -> Option<Instant> {
        self.limit_time
            .then_some(started)
            .flatten()
            .and_then(|start| {
                start.checked_add(Duration::from_secs(u64::from(
                    self.max_analyze_time_seconds,
                )))
            })
    }

    pub(crate) fn reached(
        &self,
        started: Option<Instant>,
        visits: Option<u64>,
    ) -> Option<LimitReason> {
        if self.limit_playout && visits.is_some_and(|visits| visits > self.limit_playouts) {
            Some(LimitReason::Playouts)
        } else if self
            .deadline(started)
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Some(LimitReason::Time)
        } else {
            None
        }
    }
}
