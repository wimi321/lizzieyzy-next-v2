use super::{Actor, PendingTrial};
use crate::{AccelerationError, AccelerationPhase, AppError, EngineSetupError};
pub(in crate::actor) enum Signal {
    Finished(Result<(), EngineSetupError>),
    Output { tail: Vec<String>, tuning: bool },
}

impl Actor {
    pub(in crate::actor) async fn cancel_acceleration_trial(&mut self, restart: bool) {
        let Some(mut trial) = self.trial.take() else {
            return;
        };
        let cleanup = trial.engine.clone().shutdown().await;
        if let Some(monitor) = &mut trial.monitor {
            monitor.drain();
            self.acceleration.output = monitor.lines();
            self.acceleration.backend_line = monitor.backend_line.clone();
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
                if self.acceleration.backend.is_none() {
                    return;
                }
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
        if !matches!(trial.reply, super::Reply::Acceleration(_)) {
            return result;
        }
        let Some(mut monitor) = trial.monitor.take() else {
            return result;
        };
        // Include output written just before the version response or process exit.
        monitor.drain();
        self.acceleration.output = monitor.lines();
        self.acceleration.backend_line = monitor.backend_line.clone();
        match result {
            Ok(()) => {
                // stdout Ready may arrive before a previously written stderr
                // line. Keep consuming this bounded receiver after adoption.
                monitor.finish();
                self.acceleration.monitor = Some(monitor);
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
