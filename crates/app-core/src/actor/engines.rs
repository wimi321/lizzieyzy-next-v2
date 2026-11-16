use super::{Actor, trial};
use crate::{
    AppError, EngineEntry, EngineProfile, Settings,
    engines::{Request, invalid},
};
use tokio::sync::oneshot;

struct PreparedTrial {
    entries: Vec<EngineEntry>,
    id: String,
    profile: EngineProfile,
    comparison: bool,
}

impl PreparedTrial {
    fn new(
        entries: Vec<EngineEntry>,
        id: String,
        profile: EngineProfile,
        comparison: bool,
    ) -> Self {
        Self {
            entries,
            id,
            profile,
            comparison,
        }
    }
}

impl Actor {
    pub(super) async fn engine_command(
        &mut self,
        request: Request,
        reply: oneshot::Sender<Result<Settings, AppError>>,
    ) -> bool {
        if let Err(error) = self.ensure_no_benchmark() {
            let _ = reply.send(Err(error));
            return false;
        }
        self.auto_setup.cancel();
        self.cancel_trial().await;
        let changes_comparison = match &request {
            Request::SelectComparison(_)
            | Request::RestartComparison
            | Request::CloseComparison
            | Request::CloseOthers
            | Request::CloseAll => true,
            Request::Update(id, _, _) | Request::Remove(id) => {
                self.settings.comparison_engine_id.as_ref() == Some(id)
            }
            _ => false,
        };
        let interrupted_startup = changes_comparison
            && self
                .comparison
                .trial
                .as_ref()
                .is_some_and(trial::PendingTrial::is_startup);
        if changes_comparison {
            self.cancel_comparison_trial().await;
        } else {
            self.cancel_comparison_edit_trial().await;
        }
        let result = self.prepare_engine_command(request).await;
        match result {
            Ok(Some(candidate)) => {
                if candidate.comparison && self.comparison.running.is_none() {
                    self.comparison.status = crate::EngineStatus::Starting;
                    self.publish_comparison_changed();
                }
                self.begin_target_trial(
                    candidate.entries,
                    candidate.id,
                    candidate.profile,
                    trial::Reply::Settings(reply),
                    candidate.comparison,
                )
                .await;
                false
            }
            Ok(None) => {
                let _ = reply.send(Ok(self.settings.clone()));
                true
            }
            Err(error) => {
                if interrupted_startup {
                    self.reconcile_comparison().await;
                }
                let _ = reply.send(Err(error));
                false
            }
        }
    }

    async fn prepare_engine_command(
        &mut self,
        request: Request,
    ) -> Result<Option<PreparedTrial>, AppError> {
        let mut settings = self.settings.clone();
        let mut stop = false;
        match request {
            Request::Add(name, profile) => {
                profile.validate_transport()?;
                settings.engines.push(EngineEntry {
                    id: settings.next_engine_id(),
                    name,
                    profile,
                });
            }
            Request::Update(id, name, profile) => {
                profile.validate_transport()?;
                let entry = settings
                    .engines
                    .iter_mut()
                    .find(|entry| entry.id == id)
                    .ok_or_else(|| invalid("engine ID does not exist"))?;
                entry.name = name;
                entry.profile = profile.clone();
                settings.normalize_engines()?;
                if settings.main_engine_id.as_ref() == Some(&id)
                    && settings.engine.is_some()
                    && self.settings.engine.as_ref() != Some(&profile)
                {
                    return Ok(Some(PreparedTrial::new(
                        settings.engines,
                        id,
                        profile,
                        false,
                    )));
                }
                if settings.comparison_engine_id.as_ref() == Some(&id)
                    && self.comparison.running.is_some()
                    && self
                        .settings
                        .engines
                        .iter()
                        .find(|entry| entry.id == id)
                        .is_some_and(|entry| entry.profile != profile)
                {
                    return Ok(Some(PreparedTrial::new(
                        settings.engines,
                        id,
                        profile,
                        true,
                    )));
                }
            }
            Request::Remove(id) => {
                let index = settings
                    .engines
                    .iter()
                    .position(|entry| entry.id == id)
                    .ok_or_else(|| invalid("engine ID does not exist"))?;
                settings.engines.remove(index);
                if settings.comparison_engine_id.as_ref() == Some(&id) {
                    settings.comparison_engine_id = None;
                }
                if settings.main_engine_id.as_ref() == Some(&id) {
                    settings.main_engine_id = None;
                    settings.engine = None;
                    stop = true;
                }
            }
            Request::Reorder(ids) => {
                if ids.len() != settings.engines.len() {
                    return Err(invalid("engine order must contain every ID exactly once").into());
                }
                let mut ordered = Vec::new();
                for id in ids {
                    let index = settings
                        .engines
                        .iter()
                        .position(|entry| entry.id == id)
                        .ok_or_else(|| {
                            invalid("engine order must contain every ID exactly once")
                        })?;
                    ordered.push(settings.engines.remove(index));
                }
                settings.engines = ordered;
            }
            Request::Select(id) => {
                if settings.comparison_engine_id.as_ref() == Some(&id) {
                    return Err(crate::SettingsError::SameAsComparison.into());
                }
                let profile = settings
                    .engines
                    .iter()
                    .find(|entry| entry.id == id)
                    .ok_or_else(|| invalid("engine ID does not exist"))?
                    .profile
                    .clone();
                return Ok(Some(PreparedTrial::new(
                    settings.engines,
                    id,
                    profile,
                    false,
                )));
            }
            Request::Restart => {
                let id = settings
                    .main_engine_id
                    .clone()
                    .ok_or_else(|| invalid("no main engine selected"))?;
                let profile = settings
                    .engines
                    .iter()
                    .find(|entry| entry.id == id)
                    .ok_or_else(|| invalid("engine ID does not exist"))?
                    .profile
                    .clone();
                return Ok(Some(PreparedTrial::new(
                    settings.engines,
                    id,
                    profile,
                    false,
                )));
            }
            Request::Close => {
                settings.engine = None;
                stop = true;
            }
            Request::SelectComparison(Some(id)) => {
                if settings.main_engine_id.as_ref() == Some(&id) {
                    return Err(crate::SettingsError::SameAsMain.into());
                }
                let profile = settings
                    .engines
                    .iter()
                    .find(|entry| entry.id == id)
                    .ok_or_else(|| invalid("engine ID does not exist"))?
                    .profile
                    .clone();
                if settings.display.layout_mode == crate::LayoutMode::DoubleEngine {
                    return Ok(Some(PreparedTrial::new(
                        settings.engines,
                        id,
                        profile,
                        true,
                    )));
                }
                settings.comparison_engine_id = Some(id);
            }
            Request::RestartComparison => {
                let id = settings
                    .comparison_engine_id
                    .clone()
                    .ok_or_else(|| invalid("no comparison engine selected"))?;
                let profile = settings
                    .engines
                    .iter()
                    .find(|entry| entry.id == id)
                    .ok_or_else(|| invalid("engine ID does not exist"))?
                    .profile
                    .clone();
                if settings.display.layout_mode == crate::LayoutMode::DoubleEngine {
                    return Ok(Some(PreparedTrial::new(
                        settings.engines,
                        id,
                        profile,
                        true,
                    )));
                }
            }
            Request::SelectComparison(None) | Request::CloseComparison | Request::CloseOthers => {
                settings.comparison_engine_id = None;
            }
            Request::CloseAll => {
                settings.comparison_engine_id = None;
                settings.engine = None;
                stop = true;
            }
        }
        settings = settings.normalized()?;
        settings.save(&self.directory).await?;
        let comparison_changed =
            settings.comparison_engine_id != self.settings.comparison_engine_id;
        let comparison_profile_changed = settings
            .engines
            .iter()
            .find(|entry| Some(&entry.id) == settings.comparison_engine_id.as_ref())
            .map(|entry| &entry.profile)
            != self
                .settings
                .engines
                .iter()
                .find(|entry| Some(&entry.id) == self.settings.comparison_engine_id.as_ref())
                .map(|entry| &entry.profile);
        self.settings = settings;
        if comparison_changed || comparison_profile_changed {
            self.set_comparison_fingerprint(None);
            self.stop_comparison().await?;
            self.publish_comparison_changed();
        }
        if stop {
            self.stop_engine().await?;
            self.set_summary_fingerprint(None);
            self.publish_saved_focus();
        }
        self.reconcile_comparison().await;
        self.update_focus().await;
        Ok(None)
    }

    pub(super) async fn configure_engine(
        &mut self,
        profile: Option<EngineProfile>,
    ) -> Result<(), AppError> {
        self.ensure_no_benchmark()?;
        self.auto_setup.cancel();
        self.cancel_trial().await;
        self.cancel_comparison_edit_trial().await;
        // Validate argument conflicts before changing the saved configuration.
        if let Some(profile) = &profile {
            profile.validate_transport()?;
        }
        let mut settings = self.settings.clone();
        settings.set_legacy_engine(profile);
        settings.save(&self.directory).await?;
        let changed = settings.engine != self.settings.engine;
        self.settings = settings;
        let healthy = self.running.as_ref().is_some_and(|running| {
            !matches!(
                *running.engine_status.borrow(),
                engine_session::EngineStatus::Failed { .. } | engine_session::EngineStatus::Stopped
            )
        });
        let unchanged_files =
            if let (Some(profile), Some(running)) = (&self.settings.engine, &self.running) {
                profile.fingerprint().await.ok().as_ref() == Some(&running.fingerprint)
            } else {
                false
            };
        // Reapplying a profile also detects replaced model/config files.
        if changed || !healthy || !unchanged_files {
            self.stop_engine().await?;
            self.start_engine().await;
        }
        self.update_focus().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
