use super::{Actor, Running};
use crate::{AppError, AppEvent, EngineProfile, EngineSetupError, EngineSetupProfile};
use engine_session::{EngineHandle, EngineStatus};
use std::{future::pending, path::PathBuf, time::Duration};
use tokio::{
    sync::{oneshot, watch},
    time::Instant,
};

pub(super) struct PendingTrial {
    engine: EngineHandle,
    status: watch::Receiver<EngineStatus>,
    profile: EngineProfile,
    fingerprint: String,
    deadline: Instant,
    timeout: Duration,
    reply: oneshot::Sender<Result<(), AppError>>,
}

pub(super) async fn wait(trial: &mut Option<PendingTrial>) -> Result<(), EngineSetupError> {
    let Some(trial) = trial else {
        return pending().await;
    };
    loop {
        if trial.reply.is_closed() {
            return Err(EngineSetupError::Cancelled);
        }
        match trial.status.borrow_and_update().clone() {
            EngineStatus::Ready { .. } => return Ok(()),
            EngineStatus::Failed {
                reason,
                stderr_tail,
            } => {
                return Err(EngineSetupError::Failed {
                    reason,
                    stderr_tail,
                });
            }
            EngineStatus::Stopped => return Err(EngineSetupError::Cancelled),
            EngineStatus::Starting | EngineStatus::Restarting { .. } => {}
        }
        tokio::select! {
            changed = trial.status.changed() => {
                if changed.is_err() { return Err(EngineSetupError::Cancelled); }
            }
            _ = tokio::time::sleep_until(trial.deadline) => return Err(EngineSetupError::TimedOut(trial.timeout)),
            _ = trial.reply.closed() => return Err(EngineSetupError::Cancelled),
        }
    }
}

impl Actor {
    pub(super) async fn begin_trial(
        &mut self,
        profile: EngineSetupProfile,
        source: PathBuf,
        reply: oneshot::Sender<Result<(), AppError>>,
    ) {
        self.cancel_trial().await;
        let prepared = async {
            let timeout = self.config.engine_setup_timeout;
            if timeout.is_zero() || timeout > Duration::from_secs(86400) {
                return Err(AppError::from(EngineSetupError::InvalidTimeout));
            }
            let profile = profile.resolve(&self.directory, source).await?;
            // Both startup paths share the same profile validation, override merge
            // and cache identity. The candidate never owns the old engine.
            let transport = profile.transport()?;
            let fingerprint = profile.fingerprint().await?;
            Ok((profile, transport, fingerprint, timeout))
        }
        .await;
        match prepared {
            Ok((profile, transport, fingerprint, timeout)) => {
                let engine = engine_session::spawn(transport, self.config.session.clone());
                self.trial = Some(PendingTrial {
                    status: engine.status(),
                    engine,
                    profile,
                    fingerprint,
                    deadline: Instant::now() + timeout,
                    timeout,
                    reply,
                });
            }
            Err(error) => {
                let _ = reply.send(Err(error));
            }
        }
    }

    pub(super) async fn cancel_trial(&mut self) {
        if let Some(trial) = self.trial.take() {
            let cleanup = trial.engine.shutdown().await;
            let error =
                cleanup.map_or_else(AppError::from, |()| EngineSetupError::Cancelled.into());
            let _ = trial.reply.send(Err(error));
        }
    }

    pub(super) async fn finish_trial(&mut self, outcome: Result<(), EngineSetupError>) {
        let Some(trial) = self.trial.take() else {
            return;
        };
        let result = match outcome {
            Ok(()) => self.commit_trial(&trial).await,
            Err(error) => Err(error.into()),
        };
        if result.is_err() {
            // Failed attempts never publish status events or touch the running
            // coordinator; UI errors belong to this command's response.
            let _ = trial.engine.shutdown().await;
        }
        let _ = trial.reply.send(result);
    }

    async fn commit_trial(&mut self, trial: &PendingTrial) -> Result<(), AppError> {
        if trial.reply.is_closed() {
            return Err(EngineSetupError::Cancelled.into());
        }
        if trial.profile.fingerprint().await? != trial.fingerprint {
            return Err(EngineSetupError::ChangedFiles.into());
        }
        // Filesystem work may have yielded long enough for the candidate to
        // exit. Read current readiness again at the persistence boundary.
        if trial.reply.is_closed() {
            return Err(EngineSetupError::Cancelled.into());
        }
        match trial.status.borrow().clone() {
            EngineStatus::Ready { .. } => {}
            EngineStatus::Failed {
                reason,
                stderr_tail,
            } => {
                return Err(EngineSetupError::Failed {
                    reason,
                    stderr_tail,
                }
                .into());
            }
            status => {
                return Err(EngineSetupError::Failed {
                    reason: format!("engine lost readiness before saving: {status:?}"),
                    stderr_tail: Vec::new(),
                }
                .into());
            }
        }
        let (coordinator, snapshots) = self.coordinator(&trial.engine, &trial.fingerprint)?;
        let mut settings = self.settings.clone();
        settings.engine = Some(trial.profile.clone());
        if let Err(error) = settings.save(&self.directory).await {
            let _ = coordinator.shutdown().await;
            return Err(error.into());
        }
        // This engine is already Ready. Adopt it instead of starting a second,
        // untested process after persisting the profile.
        let cleanup = self.stop_engine().await;
        self.settings = settings;
        self.set_summary_fingerprint(Some(trial.fingerprint.clone()));
        let engine_status = trial.engine.status();
        self.output.send(AppEvent::EngineStatusChanged(
            engine_status.borrow().clone().into(),
        ));
        self.running = Some(Running {
            engine: trial.engine.clone(),
            analysis_status: coordinator.status(),
            coordinator,
            snapshots,
            engine_status,
            fingerprint: trial.fingerprint.clone(),
        });
        self.update_focus().await;
        // A previous session's cleanup failure must not discard the new engine
        // after the settings transaction has already succeeded.
        if let Err(error) = cleanup {
            self.analysis_failed(error.to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
