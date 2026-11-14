use super::super::{Actor, Running};
use crate::{AppError, RemoteEngineConfig};

impl Actor {
    pub(in crate::actor) fn remote_identity_current(&self) -> bool {
        let Some(remote) = self
            .settings
            .engine
            .as_ref()
            .and_then(|p| p.remote.as_ref())
        else {
            return true;
        };
        let Some(running) = &self.running else {
            return false;
        };
        let saved = match remote {
            RemoteEngineConfig::WebSocket { version: saved, .. }
            | RemoteEngineConfig::Ssh { version: saved, .. } => saved,
        };
        match &*running.engine_status.borrow() {
            engine_session::EngineStatus::Ready { version } => saved.as_ref() == Some(version),
            // Queued results/terminal events during backoff still belong to
            // the last confirmed version. Consume them normally so a finished
            // ReviewRun cannot spin forever on an ignored None. The initial
            // remote trial never constructs Running before its first Ready.
            _ => saved.is_some(),
        }
    }

    /// Check the watch value before consuming either result stream: a result
    /// can be selected before the Ready notification after a remote upgrade.
    pub(in crate::actor) async fn refresh_remote_identity(&mut self) -> bool {
        if self.remote_identity_current() {
            return true;
        }
        let Some(running) = &self.running else {
            return false;
        };
        let status = running.engine_status.borrow().clone();
        if let engine_session::EngineStatus::Ready { version } = status
            && let Err(error) = self.replace_remote_identity(&version).await
        {
            self.analysis_failed(error.to_string());
        }
        // Any pending snapshot came from the previous coordinator namespace.
        false
    }

    async fn replace_remote_identity(&mut self, version: &str) -> Result<(), AppError> {
        let Some(profile) = self.settings.engine.as_ref().filter(|p| p.remote.is_some()) else {
            return Ok(());
        };
        let profile = profile.with_remote_version(version);
        let fingerprint = profile.fingerprint().await?;
        self.flush_cached_analysis().await;
        self.cancel_review().await;
        let Some(running) = self.running.take() else {
            return Ok(());
        };
        running.coordinator.shutdown().await?;
        let (coordinator, snapshots) = self.coordinator(&running.engine, &fingerprint)?;
        self.settings.set_legacy_engine(Some(profile));
        self.output.clear_analysis();
        self.focus = None;
        self.set_summary_fingerprint(Some(fingerprint.clone()));
        self.running = Some(Running {
            analysis_status: coordinator.status(),
            coordinator,
            snapshots,
            fingerprint,
            ..running
        });
        self.update_focus().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
