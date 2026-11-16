use super::super::{Actor, Running, trial};
use crate::{AppError, EngineStatus, LayoutMode, RemoteEngineConfig, Settings};
use analysis::{AnalysisHandle, AnalysisSubscription};
use engine_session::EngineHandle;

impl Actor {
    /// The layout and selection are the sole durable on/off switch. Failed
    /// starts are retried only after an explicit selection/restart or re-entry.
    pub(in crate::actor) async fn reconcile_comparison(&mut self) {
        if self.settings.display.layout_mode != LayoutMode::DoubleEngine
            || self.settings.comparison_engine_id.is_none()
        {
            if (self.comparison.running.is_some()
                || self.comparison.trial.is_some()
                || self.comparison.status != EngineStatus::Stopped)
                && let Err(error) = self.stop_comparison().await
            {
                self.comparison_trial_failed(&error);
            }
            return;
        }
        if self.comparison.running.is_some()
            || self.comparison.trial.is_some()
            || matches!(self.comparison.status, EngineStatus::Failed { .. })
        {
            return;
        }
        let Some(id) = self.settings.comparison_engine_id.clone() else {
            return;
        };
        let Some(profile) = self
            .settings
            .engines
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.profile.clone())
        else {
            return;
        };
        self.comparison.status = EngineStatus::Starting;
        self.publish_comparison_changed();
        self.begin_target_trial(
            self.settings.engines.clone(),
            id,
            profile,
            trial::Reply::Startup,
            true,
        )
        .await;
    }

    pub(in crate::actor) async fn stop_comparison(&mut self) -> Result<(), AppError> {
        let changed = self.comparison.running.is_some()
            || self.comparison.trial.is_some()
            || self.comparison.status != EngineStatus::Stopped;
        self.cancel_comparison_trial().await;
        self.flush_cached_analysis().await;
        self.comparison.latest = None;
        self.output.clear_comparison_analysis();
        let result = if let Some(running) = self.comparison.running.take() {
            let analysis = running.coordinator.shutdown().await.map_err(AppError::from);
            let engine = running.engine.shutdown().await.map_err(AppError::from);
            analysis.and(engine)
        } else {
            Ok(())
        };
        self.comparison.status = EngineStatus::Stopped;
        if changed {
            self.publish_comparison_changed();
        }
        result
    }

    pub(in crate::actor) async fn comparison_trial_signal(&mut self, signal: trial::TrialSignal) {
        if let trial::TrialSignal::Finished(result) = signal {
            self.finish_comparison_trial(result).await;
        }
    }

    pub(in crate::actor) fn comparison_trial_failed(&mut self, error: &AppError) {
        // A failed replacement is a command error; the existing session remains
        // live and its separately reported status continues to describe it.
        if self.comparison.running.is_none() {
            self.comparison.status = EngineStatus::Failed {
                reason: error.to_string(),
                stderr_tail: Vec::new(),
            };
        }
        self.publish_comparison_changed();
    }

    pub(in crate::actor) async fn adopt_comparison_trial(
        &mut self,
        settings: Settings,
        engine: EngineHandle,
        fingerprint: String,
        coordinator: AnalysisHandle,
        snapshots: AnalysisSubscription,
    ) -> Result<(), AppError> {
        let cleanup = self.stop_comparison().await;
        self.settings = settings;
        self.set_comparison_fingerprint(Some(fingerprint.clone()));
        let engine_status = engine.status();
        self.comparison.status = engine_status.borrow().clone().into();
        self.comparison.running = Some(Running {
            engine,
            analysis_status: coordinator.status(),
            coordinator,
            snapshots,
            engine_status,
            fingerprint,
        });
        self.update_focus().await;
        self.publish_comparison_changed();
        if let Err(error) = cleanup {
            // Saving is the commit boundary. Retain the already adopted engine
            // even if the old process failed to acknowledge cleanup.
            tracing::warn!(%error, "previous comparison engine cleanup failed");
        }
        Ok(())
    }

    /// A remote upgrade changes its evaluation namespace before any queued
    /// response is accepted, exactly as for the main engine.
    pub(in crate::actor) async fn refresh_comparison_identity(&mut self) -> bool {
        let Some(id) = &self.settings.comparison_engine_id else {
            return true;
        };
        let Some(profile) = self
            .settings
            .engines
            .iter()
            .find(|entry| &entry.id == id)
            .map(|entry| entry.profile.clone())
        else {
            return true;
        };
        let Some(remote) = &profile.remote else {
            return true;
        };
        let Some(running) = &self.comparison.running else {
            return false;
        };
        let saved = match remote {
            RemoteEngineConfig::WebSocket { version, .. }
            | RemoteEngineConfig::Ssh { version, .. }
            | RemoteEngineConfig::Zhizi { version, .. } => version,
        };
        let status = running.engine_status.borrow().clone();
        let engine_session::EngineStatus::Ready { version } = status else {
            return saved.is_some();
        };
        if saved.as_ref() == Some(&version) {
            return true;
        }
        let profile = profile.with_remote_version(&version);
        if let Err(error) = self.replace_comparison_identity(profile).await {
            self.comparison.status = EngineStatus::Failed {
                reason: error.to_string(),
                stderr_tail: Vec::new(),
            };
            self.publish_comparison_changed();
        }
        false
    }

    async fn replace_comparison_identity(
        &mut self,
        profile: crate::EngineProfile,
    ) -> Result<(), AppError> {
        let fingerprint = profile.fingerprint().await?;
        self.flush_cached_analysis().await;
        let Some(mut running) = self.comparison.running.take() else {
            return Ok(());
        };
        let (coordinator, snapshots) = match self.coordinator(&running.engine, &fingerprint) {
            Ok(value) => value,
            Err(error) => {
                self.comparison.running = Some(running);
                return Err(error);
            }
        };
        let previous = std::mem::replace(&mut running.coordinator, coordinator);
        running.analysis_status = running.coordinator.status();
        running.snapshots = snapshots;
        running.fingerprint = fingerprint.clone();
        if let Some(entry) = self
            .settings
            .engines
            .iter_mut()
            .find(|entry| Some(&entry.id) == self.settings.comparison_engine_id.as_ref())
        {
            entry.profile = profile;
        }
        self.comparison.latest = None;
        self.set_comparison_fingerprint(Some(fingerprint));
        self.comparison.status = running.engine_status.borrow().clone().into();
        self.comparison.running = Some(running);
        previous.shutdown().await?;
        self.update_focus().await;
        self.publish_comparison_changed();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
