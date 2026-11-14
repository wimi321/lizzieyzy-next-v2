use super::{Actor, Running};
use crate::{
    AppError, AppEvent, EngineEntry, EngineProfile, EngineSetupError, EngineSetupProfile, Settings,
};
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
    reply: Reply,
    list_edit: Option<(Vec<EngineEntry>, String)>,
}

impl PendingTrial {
    pub(super) fn is_zhizi(&self) -> bool {
        matches!(
            self.profile.remote,
            Some(crate::RemoteEngineConfig::Zhizi { .. })
        )
    }
}

pub(super) enum Reply {
    Startup,
    Manual(oneshot::Sender<Result<(), AppError>>),
    Auto(oneshot::Sender<Result<EngineProfile, AppError>>),
    Settings(oneshot::Sender<Result<crate::Settings, AppError>>),
}

impl Reply {
    fn is_closed(&self) -> bool {
        match self {
            Self::Startup => false,
            Self::Manual(reply) => reply.is_closed(),
            Self::Auto(reply) => reply.is_closed(),
            Self::Settings(reply) => reply.is_closed(),
        }
    }

    async fn closed(&mut self) {
        match self {
            Self::Startup => pending().await,
            Self::Manual(reply) => reply.closed().await,
            Self::Auto(reply) => reply.closed().await,
            Self::Settings(reply) => reply.closed().await,
        }
    }

    fn send(self, result: Result<(EngineProfile, Settings), AppError>) {
        match self {
            Self::Startup => {}
            Self::Manual(reply) => {
                let _ = reply.send(result.map(|_| ()));
            }
            Self::Auto(reply) => {
                let _ = reply.send(result.map(|(profile, _)| profile));
            }
            Self::Settings(reply) => {
                let _ = reply.send(result.map(|(_, settings)| settings));
            }
        }
    }
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
        self.begin_trial_with_options(profile, source, Reply::Manual(reply), None)
            .await;
    }

    pub(super) async fn begin_trial_with_options(
        &mut self,
        profile: EngineSetupProfile,
        source: PathBuf,
        reply: Reply,
        auto: Option<(Option<String>, Instant)>,
    ) {
        if let Err(error) = self.ensure_no_benchmark() {
            reply.send(Err(error));
            return;
        }
        self.cancel_trial().await;
        let prepared = async {
            let timeout =
                auto.as_ref()
                    .map_or(self.config.engine_setup_timeout, |(_, deadline)| {
                        deadline
                            .saturating_duration_since(Instant::now())
                            .min(crate::auto_setup::TRIAL_TIMEOUT)
                    });
            if timeout.is_zero() || timeout > Duration::from_secs(86400) {
                return Err(AppError::from(EngineSetupError::InvalidTimeout));
            }
            let mut profile = profile.resolve(&self.directory, source).await?;
            if let Some((name, _)) = &auto {
                profile.canonical_model_name = name.clone();
            }
            // Both startup paths share the same profile validation, override merge
            // and cache identity. The candidate never owns the old engine.
            let fingerprint = profile.fingerprint().await?;
            let benchmark = self.settings.benchmarks.get(&fingerprint);
            let mut transport = profile.transport_with_benchmark(benchmark)?;
            self.with_runtime_environment(&profile, &mut transport)
                .await?;
            Ok((profile, transport, fingerprint, timeout))
        };
        let prepared = if let Some((_, deadline)) = &auto {
            tokio::time::timeout_at(*deadline, prepared)
                .await
                .unwrap_or_else(|_| {
                    Err(EngineSetupError::TimedOut(crate::auto_setup::TOTAL_TIMEOUT).into())
                })
        } else {
            prepared.await
        };
        match prepared {
            Ok((profile, transport, fingerprint, timeout)) => {
                let mut session = self.session_config();
                if auto.is_some() {
                    session.max_restarts = 0;
                    session.shutdown_timeout = session.shutdown_timeout.min(Duration::from_secs(2));
                }
                let deadline = auto
                    .as_ref()
                    .map_or(Instant::now() + timeout, |(_, deadline)| {
                        (*deadline).min(Instant::now() + timeout)
                    });
                if reply.is_closed() || deadline <= Instant::now() {
                    reply.send(Err(EngineSetupError::Cancelled.into()));
                    return;
                }
                let engine = engine_session::spawn(transport, session);
                self.trial = Some(PendingTrial {
                    status: engine.status(),
                    engine,
                    profile,
                    fingerprint,
                    deadline,
                    timeout,
                    reply,
                    list_edit: None,
                });
            }
            Err(error) => {
                reply.send(Err(error));
            }
        }
    }

    pub(super) async fn begin_list_trial(
        &mut self,
        engines: Vec<EngineEntry>,
        id: String,
        profile: EngineProfile,
        reply: Reply,
    ) {
        let prepared = async {
            let timeout = self.config.engine_setup_timeout;
            if timeout.is_zero() || timeout > Duration::from_secs(86400) {
                return Err(AppError::from(EngineSetupError::InvalidTimeout));
            }
            let (transport, fingerprint) = self.prepare_engine_transport(&profile).await?;
            Ok((fingerprint, transport, timeout))
        }
        .await;
        match prepared {
            Ok((fingerprint, transport, timeout)) => {
                if reply.is_closed() {
                    return;
                }
                let engine = engine_session::spawn(transport, self.session_config());
                self.trial = Some(PendingTrial {
                    status: engine.status(),
                    engine,
                    profile,
                    fingerprint,
                    deadline: Instant::now() + timeout,
                    timeout,
                    reply,
                    list_edit: Some((engines, id)),
                });
            }
            Err(error) => {
                if matches!(reply, Reply::Startup) {
                    self.set_summary_fingerprint(None);
                    self.failed(error.to_string());
                }
                reply.send(Err(error));
            }
        }
    }

    pub(super) async fn cancel_trial(&mut self) {
        if let Some(trial) = self.trial.take() {
            self.remote.rollback();
            let cleanup = trial.engine.shutdown().await;
            let error =
                cleanup.map_or_else(AppError::from, |()| EngineSetupError::Cancelled.into());
            trial.reply.send(Err(error));
        }
    }

    pub(super) async fn finish_trial(&mut self, outcome: Result<(), EngineSetupError>) {
        let Some(mut trial) = self.trial.take() else {
            return;
        };
        let result = match outcome {
            Ok(()) => self.commit_trial(&mut trial).await,
            Err(error) => Err(error.into()),
        };
        if let Err(error) = &result
            && matches!(trial.reply, Reply::Startup)
        {
            self.set_summary_fingerprint(None);
            self.failed(error.to_string());
        }
        if result.is_ok() {
            self.remote.commit().await;
        } else {
            self.remote.rollback();
        }
        if result.is_err() {
            // Failed attempts never publish status events or touch the running
            // coordinator; UI errors belong to this command's response.
            let _ = trial.engine.shutdown().await;
        }
        trial
            .reply
            .send(result.map(|()| (trial.profile, self.settings.clone())));
    }

    async fn commit_trial(&mut self, trial: &mut PendingTrial) -> Result<(), AppError> {
        if trial.reply.is_closed() {
            return Err(EngineSetupError::Cancelled.into());
        }
        if Instant::now() >= trial.deadline {
            return Err(EngineSetupError::TimedOut(trial.timeout).into());
        }
        let fingerprint = tokio::time::timeout_at(trial.deadline, trial.profile.fingerprint())
            .await
            .map_err(|_| EngineSetupError::TimedOut(trial.timeout))??;
        if fingerprint != trial.fingerprint {
            return Err(EngineSetupError::ChangedFiles.into());
        }
        // Filesystem work may have yielded long enough for the candidate to
        // exit. Read current readiness again at the persistence boundary.
        if trial.reply.is_closed() {
            return Err(EngineSetupError::Cancelled.into());
        }
        if Instant::now() >= trial.deadline {
            return Err(EngineSetupError::TimedOut(trial.timeout).into());
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
        let ready_status = trial.status.borrow().clone();
        if trial.profile.remote.is_some()
            && let EngineStatus::Ready { version } = ready_status
        {
            trial.profile = trial.profile.with_remote_version(&version);
            trial.fingerprint = trial.profile.fingerprint().await?;
        }
        let (coordinator, snapshots) = self.coordinator(&trial.engine, &trial.fingerprint)?;
        let mut settings = self.settings.clone();
        if let Some((engines, id)) = &trial.list_edit {
            settings.engines = engines.clone();
            if let Some(entry) = settings.engines.iter_mut().find(|entry| &entry.id == id) {
                entry.profile = trial.profile.clone();
            }
            settings.main_engine_id = Some(id.clone());
            settings.engine = Some(trial.profile.clone());
        } else {
            settings.set_legacy_engine(Some(trial.profile.clone()));
        }
        // Recheck immediately before beginning the atomic settings write. Once
        // it succeeds, this transaction owns the saved profile and adopts it.
        if trial.reply.is_closed() {
            let _ = coordinator.shutdown().await;
            return Err(EngineSetupError::Cancelled.into());
        }
        if Instant::now() >= trial.deadline {
            let _ = coordinator.shutdown().await;
            return Err(EngineSetupError::TimedOut(trial.timeout).into());
        }
        if let Err(error) = settings.save(&self.directory).await {
            let _ = coordinator.shutdown().await;
            return Err(error.into());
        }
        // This engine is already Ready. Adopt it instead of starting a second,
        // untested process after persisting the profile.
        let cleanup = self.stop_engine().await;
        self.startup_local_fallback = false;
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
