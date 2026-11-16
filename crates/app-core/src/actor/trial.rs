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

mod acceleration;
pub(super) use acceleration::Signal as TrialSignal;

pub(super) struct PendingTrial {
    monitor: Option<crate::acceleration::monitor::Monitor>,
    engine: EngineHandle,
    status: watch::Receiver<EngineStatus>,
    profile: EngineProfile,
    fingerprint: String,
    deadline: Instant,
    timeout: Duration,
    retry_backoff: (Duration, Duration),
    reply: Reply,
    list_edit: Option<(Vec<EngineEntry>, String)>,
    list_base: Option<EngineEntry>,
    comparison: bool,
    owns_remote_edit: bool,
}

impl PendingTrial {
    pub(super) fn is_startup(&self) -> bool {
        matches!(self.reply, Reply::Startup)
    }

    pub(super) fn owns_remote_edit(&self) -> bool {
        self.owns_remote_edit
    }

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
    Acceleration(oneshot::Sender<Result<crate::Settings, AppError>>),
}

impl Reply {
    fn is_closed(&self) -> bool {
        match self {
            Self::Startup => false,
            Self::Manual(reply) => reply.is_closed(),
            Self::Auto(reply) => reply.is_closed(),
            Self::Settings(reply) | Self::Acceleration(reply) => reply.is_closed(),
        }
    }

    async fn closed(&mut self) {
        match self {
            Self::Startup => pending().await,
            Self::Manual(reply) => reply.closed().await,
            Self::Auto(reply) => reply.closed().await,
            Self::Settings(reply) | Self::Acceleration(reply) => reply.closed().await,
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
            Self::Settings(reply) | Self::Acceleration(reply) => {
                let _ = reply.send(result.map(|(_, settings)| settings));
            }
        }
    }
}

pub(super) async fn wait(trial: &mut Option<PendingTrial>) -> TrialSignal {
    let Some(trial) = trial else {
        return pending().await;
    };
    loop {
        if trial.reply.is_closed() {
            return TrialSignal::Finished(Err(EngineSetupError::Cancelled));
        }
        let status = trial.status.borrow_and_update().clone();
        if let Some(monitor) = &mut trial.monitor {
            if let EngineStatus::Restarting { attempt, .. } = &status {
                // Restarting is announced before the session's backoff. Do
                // not consume the next process's base timeout during that wait.
                let delay = trial
                    .retry_backoff
                    .0
                    .saturating_mul(
                        1u32.checked_shl(attempt.saturating_sub(1))
                            .unwrap_or(u32::MAX),
                    )
                    .min(trial.retry_backoff.1);
                monitor.restarted(*attempt, Instant::now() + delay);
            }
            monitor.drain();
            trial.deadline = monitor.clock.deadline();
        }
        if Instant::now() >= trial.deadline {
            return TrialSignal::Finished(Err(EngineSetupError::TimedOut(trial.timeout)));
        }
        match status {
            EngineStatus::Ready { .. } => return TrialSignal::Finished(Ok(())),
            EngineStatus::Failed {
                reason,
                stderr_tail,
            } => {
                return TrialSignal::Finished(Err(EngineSetupError::Failed {
                    reason,
                    stderr_tail,
                }));
            }
            EngineStatus::Stopped => {
                return TrialSignal::Finished(Err(EngineSetupError::Cancelled));
            }
            EngineStatus::Starting | EngineStatus::Restarting { .. } => {}
        }
        tokio::select! {
            changed = trial.status.changed() => {
                if changed.is_err() { return TrialSignal::Finished(Err(EngineSetupError::Cancelled)); }
            }
            _ = tokio::time::sleep_until(trial.deadline) => return TrialSignal::Finished(Err(EngineSetupError::TimedOut(trial.timeout))),
            _ = trial.reply.closed() => return TrialSignal::Finished(Err(EngineSetupError::Cancelled)),
            tuning = async {
                match &mut trial.monitor {
                    Some(monitor) => monitor.next().await,
                    None => pending().await,
                }
            } => {
                if let Some(monitor) = &trial.monitor {
                    trial.deadline = monitor.clock.deadline();
                    return TrialSignal::Output { tail: monitor.lines(), tuning };
                }
            }
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
        self.cancel_comparison_edit_trial().await;
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
                let started = Instant::now();
                let maximum = auto
                    .as_ref()
                    .map_or(session.max_startup_duration, |(_, limit)| {
                        limit
                            .saturating_duration_since(started)
                            .min(session.max_startup_duration)
                    });
                session.startup_timeout = timeout;
                session.max_startup_duration = maximum;
                let retry_backoff = (session.restart_backoff, session.max_restart_backoff);
                let (engine, stderr) = engine_session::spawn_with_stderr(transport, session);
                let monitor =
                    crate::acceleration::monitor::Monitor::new(stderr, started, timeout, maximum);
                let deadline = monitor.clock.deadline();
                self.trial = Some(PendingTrial {
                    monitor: Some(monitor),
                    status: engine.status(),
                    engine,
                    profile,
                    fingerprint,
                    deadline,
                    timeout,
                    retry_backoff,
                    reply,
                    list_edit: None,
                    list_base: None,
                    comparison: false,
                    owns_remote_edit: false,
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
        self.begin_target_trial(engines, id, profile, reply, false)
            .await;
    }

    pub(super) async fn begin_target_trial(
        &mut self,
        engines: Vec<EngineEntry>,
        id: String,
        profile: EngineProfile,
        reply: Reply,
        comparison: bool,
    ) {
        let is_acceleration = matches!(reply, Reply::Acceleration(_));
        let owns_remote_edit =
            !matches!(reply, Reply::Startup) && self.remote.pending_for(&profile);
        let list_base = self
            .settings
            .engines
            .iter()
            .find(|entry| entry.id == id)
            .cloned();
        let prepared = async {
            let timeout = self.config.engine_setup_timeout;
            if timeout.is_zero() || timeout > Duration::from_secs(86400) {
                return Err(AppError::from(EngineSetupError::InvalidTimeout));
            }
            let (transport, fingerprint) = self.prepare_engine_transport(&profile).await?;
            let timeout = if is_acceleration {
                let backend = self
                    .acceleration
                    .backend
                    .ok_or(crate::AccelerationError::Busy)?;
                crate::acceleration::deadline::timeout(backend, &self.config)
            } else {
                timeout
            };
            Ok((fingerprint, transport, timeout))
        }
        .await;
        match prepared {
            Ok((fingerprint, transport, timeout)) => {
                if reply.is_closed() {
                    if is_acceleration {
                        let _ = self
                            .acceleration_failed(EngineSetupError::Cancelled.into(), true)
                            .await;
                    }
                    return;
                }
                let mut session = self.session_config();
                if is_acceleration {
                    session.max_restarts = 0;
                    session.shutdown_timeout = session.shutdown_timeout.min(Duration::from_secs(2));
                }
                session.startup_timeout = timeout;
                let maximum = session.max_startup_duration;
                let started = Instant::now();
                let retry_backoff = (session.restart_backoff, session.max_restart_backoff);
                let (engine, stderr) = engine_session::spawn_with_stderr(transport, session);
                let monitor =
                    crate::acceleration::monitor::Monitor::new(stderr, started, timeout, maximum);
                let deadline = monitor.clock.deadline();
                let pending = Some(PendingTrial {
                    monitor: Some(monitor),
                    status: engine.status(),
                    engine,
                    profile,
                    fingerprint,
                    deadline,
                    timeout,
                    retry_backoff,
                    reply,
                    list_edit: Some((engines, id)),
                    list_base,
                    comparison,
                    owns_remote_edit,
                });
                if comparison {
                    self.comparison.trial = pending;
                } else {
                    self.trial = pending;
                }
            }
            Err(mut error) => {
                if comparison {
                    self.comparison_trial_failed(&error);
                } else if matches!(reply, Reply::Startup) {
                    self.set_summary_fingerprint(None);
                    self.failed(error.to_string());
                }
                if is_acceleration {
                    error = self.acceleration_failed(error, true).await;
                }
                reply.send(Err(error));
            }
        }
    }

    pub(super) async fn cancel_comparison_trial(&mut self) {
        if let Some(trial) = self.comparison.trial.take() {
            if trial.owns_remote_edit {
                self.remote.rollback();
            }
            let cleanup = trial.engine.shutdown().await;
            trial.reply.send(Err(
                cleanup.map_or_else(AppError::from, |()| EngineSetupError::Cancelled.into())
            ));
            self.comparison.status = self
                .comparison
                .running
                .as_ref()
                .map(|running| running.engine_status.borrow().clone().into())
                .unwrap_or(crate::EngineStatus::Stopped);
            self.publish_comparison_changed();
        }
    }

    pub(super) async fn cancel_comparison_edit_trial(&mut self) {
        // An independent main operation must not orphan the selected secondary
        // engine while its layout-driven startup is still awaiting readiness.
        if !self
            .comparison
            .trial
            .as_ref()
            .is_some_and(PendingTrial::is_startup)
        {
            self.cancel_comparison_trial().await;
        }
    }

    pub(super) async fn cancel_trial(&mut self) {
        if let Some(mut trial) = self.trial.take() {
            if trial.owns_remote_edit {
                self.remote.rollback();
            }
            if let Some(monitor) = &mut trial.monitor {
                monitor.drain();
                if matches!(trial.reply, Reply::Acceleration(_)) {
                    self.acceleration.output = monitor.lines();
                }
            }
            let cleanup = trial.engine.shutdown().await;
            let error = if matches!(trial.reply, Reply::Acceleration(_)) {
                acceleration::cancelled(self.acceleration.output.clone())
            } else {
                cleanup.map_or_else(AppError::from, |()| EngineSetupError::Cancelled.into())
            };
            trial.reply.send(Err(error));
        }
    }

    pub(super) async fn finish_trial(&mut self, outcome: Result<(), EngineSetupError>) {
        let Some(trial) = self.trial.take() else {
            return;
        };
        self.finish_pending_trial(trial, outcome).await;
    }

    pub(super) async fn finish_comparison_trial(&mut self, outcome: Result<(), EngineSetupError>) {
        let Some(trial) = self.comparison.trial.take() else {
            return;
        };
        self.finish_pending_trial(trial, outcome).await;
    }

    async fn finish_pending_trial(
        &mut self,
        mut trial: PendingTrial,
        outcome: Result<(), EngineSetupError>,
    ) {
        let mut result = match outcome {
            Ok(()) => self.commit_trial(&mut trial).await,
            Err(error) => Err(error.into()),
        };
        if let Err(error) = &result {
            if trial.comparison {
                self.comparison_trial_failed(error);
            } else if matches!(trial.reply, Reply::Startup) {
                self.set_summary_fingerprint(None);
                self.failed(error.to_string());
            }
        }
        if trial.owns_remote_edit {
            if result.is_ok() {
                self.remote.commit().await;
            } else {
                self.remote.rollback();
            }
        }
        let mut reaped = true;
        if result.is_err() {
            // Keep candidate diagnostics local to this transaction.
            if let Err(error) = trial.engine.clone().shutdown().await {
                reaped = false;
                if matches!(trial.reply, Reply::Acceleration(_)) {
                    result = Err(crate::AccelerationError::Failed {
                        reason: format!(
                            "candidate cleanup failed; previous engine was not restarted: {error}"
                        ),
                        stderr_tail: Vec::new(),
                    }
                    .into());
                }
            }
        }
        let result = self
            .complete_acceleration_trial(&mut trial, result, reaped)
            .await;
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
        let mut settings = self.settings.clone();
        if let Some((engines, id)) = &trial.list_edit {
            let planned = engines
                .iter()
                .find(|entry| &entry.id == id)
                .ok_or(EngineSetupError::Cancelled)?;
            let entry = settings
                .engines
                .iter_mut()
                .find(|entry| &entry.id == id)
                .ok_or(EngineSetupError::Cancelled)?;
            if trial.list_base.as_ref() != Some(entry) {
                return Err(EngineSetupError::ChangedFiles.into());
            }
            // Main and comparison startup trials can finish in either order.
            // Commit only this candidate; never restore its stale peer list
            // (in particular a remote peer's newly confirmed version).
            entry.name = planned.name.clone();
            entry.profile = trial.profile.clone();
            if trial.comparison {
                settings.comparison_engine_id = Some(id.clone());
            } else {
                settings.main_engine_id = Some(id.clone());
                settings.engine = Some(trial.profile.clone());
            }
        } else {
            settings.set_legacy_engine(Some(trial.profile.clone()));
        }
        let (coordinator, snapshots) = self.coordinator(&trial.engine, &trial.fingerprint)?;
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
        if trial.comparison {
            return self
                .adopt_comparison_trial(
                    settings,
                    trial.engine.clone(),
                    trial.fingerprint.clone(),
                    coordinator,
                    snapshots,
                )
                .await;
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
        self.reconcile_comparison().await;
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
