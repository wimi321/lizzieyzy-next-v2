use super::{Actor, PendingTrial};
use crate::{
    AccelerationBackend, AccelerationError, AccelerationPhase, AppError, EngineSetupError,
    acceleration::{deadline::StartupDeadline, log::Observation, output::is_tuning_line},
};
use tokio::time::Instant;

pub(super) struct Monitor {
    pub observation: Observation,
    pub clock: StartupDeadline,
}

pub(in crate::actor) enum Signal {
    Finished(Result<(), EngineSetupError>),
    Output { tail: Vec<String>, tuning: bool },
}

impl Monitor {
    pub async fn new(
        backend: AccelerationBackend,
        config: &crate::AppConfig,
    ) -> Result<Self, AppError> {
        let now = Instant::now();
        let clock = StartupDeadline::new(backend, now);
        #[cfg(test)]
        let clock = config.acceleration_timeout.map_or(clock, |timeout| {
            StartupDeadline::with_timeout(backend, now, timeout)
        });
        #[cfg(not(test))]
        let _ = config;
        Ok(Self {
            observation: Observation::new().await?,
            clock,
        })
    }

    pub async fn read(&mut self) -> Result<(Vec<String>, bool), EngineSetupError> {
        let lines = self
            .observation
            .read()
            .await
            .map_err(|error| EngineSetupError::Failed {
                reason: format!("cannot read engine startup log: {error}"),
                stderr_tail: self.observation.lines(),
            })?;
        let mut tuning = false;
        for line in lines {
            if is_tuning_line(&line) && self.clock.on_tuning(Instant::now()) {
                tuning = true;
            }
        }
        Ok((self.observation.lines(), tuning))
    }
}

impl Actor {
    pub(in crate::actor) async fn cancel_acceleration_trial(&mut self, restart: bool) {
        let Some(mut trial) = self.trial.take() else {
            return;
        };
        let cleanup = trial.engine.clone().shutdown().await;
        if let Some(monitor) = &mut trial.acceleration {
            let _ = monitor.read().await;
            monitor.observation.finish();
            self.acceleration.output = monitor.observation.lines();
            self.acceleration.backend_line = monitor.observation.backend_line.clone();
        }
        let reaped = cleanup.is_ok();
        let error = cleanup.map_or_else(
            |error| {
                AccelerationError::Failed {
                    reason: format!(
                        "candidate cleanup failed; previous engine was not restarted: {error}"
                    ),
                    stderr_tail: self.acceleration.output.clone(),
                }
                .into()
            },
            |()| EngineSetupError::Cancelled.into(),
        );
        let error = self.acceleration_failed(error, restart && reaped).await;
        trial.reply.send(Err(error));
    }

    pub(in crate::actor) async fn trial_signal(&mut self, signal: Signal) {
        match signal {
            Signal::Finished(result) => self.finish_trial(result).await,
            Signal::Output { tail, tuning } => {
                self.acceleration.output = tail;
                if tuning {
                    self.publish_acceleration(AccelerationPhase::Tuning);
                }
            }
        }
    }

    pub(super) async fn complete_acceleration_trial(
        &mut self,
        trial: &mut PendingTrial,
        result: Result<(), AppError>,
        reaped: bool,
    ) -> Result<(), AppError> {
        let Some(mut monitor) = trial.acceleration.take() else {
            return result;
        };
        // Include output written just before the version response or process exit.
        let _ = monitor.read().await;
        monitor.observation.finish();
        self.acceleration.output = monitor.observation.lines();
        self.acceleration.backend_line = monitor.observation.backend_line.clone();
        match result {
            Ok(()) => {
                self.acceleration.log = Some(monitor.observation);
                self.publish_acceleration(AccelerationPhase::Ready);
                self.acceleration.backend = None;
                Ok(())
            }
            Err(error) => Err(self.acceleration_failed(error, reaped).await),
        }
    }
}

pub(super) fn cancelled(tail: Vec<String>) -> AppError {
    AccelerationError::Cancelled { stderr_tail: tail }.into()
}
