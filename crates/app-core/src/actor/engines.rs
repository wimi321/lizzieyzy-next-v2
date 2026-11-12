use super::{Actor, trial};
use crate::{
    AppError, EngineEntry, EngineProfile, Settings,
    engines::{Request, invalid},
};
use tokio::sync::oneshot;

impl Actor {
    pub(super) async fn engine_command(
        &mut self,
        request: Request,
        reply: oneshot::Sender<Result<Settings, AppError>>,
    ) {
        if let Err(error) = self.ensure_no_benchmark() {
            let _ = reply.send(Err(error));
            return;
        }
        self.auto_setup.cancel();
        self.cancel_trial().await;
        let result = self.prepare_engine_command(request).await;
        match result {
            Ok(Some((entries, id, profile))) => {
                self.begin_list_trial(entries, id, profile, trial::Reply::Settings(reply))
                    .await;
            }
            Ok(None) => {
                let _ = reply.send(Ok(self.settings.clone()));
            }
            Err(error) => {
                let _ = reply.send(Err(error));
            }
        }
    }

    async fn prepare_engine_command(
        &mut self,
        request: Request,
    ) -> Result<Option<(Vec<EngineEntry>, String, EngineProfile)>, AppError> {
        let mut settings = self.settings.clone();
        let mut stop = false;
        match request {
            Request::Add(name, profile) => {
                profile.transport()?;
                settings.engines.push(EngineEntry {
                    id: settings.next_engine_id(),
                    name,
                    profile,
                });
            }
            Request::Update(id, name, profile) => {
                profile.transport()?;
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
                    return Ok(Some((settings.engines, id, profile)));
                }
            }
            Request::Remove(id) => {
                let index = settings
                    .engines
                    .iter()
                    .position(|entry| entry.id == id)
                    .ok_or_else(|| invalid("engine ID does not exist"))?;
                settings.engines.remove(index);
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
                let profile = settings
                    .engines
                    .iter()
                    .find(|entry| entry.id == id)
                    .ok_or_else(|| invalid("engine ID does not exist"))?
                    .profile
                    .clone();
                return Ok(Some((settings.engines, id, profile)));
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
                return Ok(Some((settings.engines, id, profile)));
            }
            Request::Close => {
                settings.engine = None;
                stop = true;
            }
        }
        settings = settings.normalized()?;
        settings.save(&self.directory).await?;
        self.settings = settings;
        if stop {
            self.stop_engine().await?;
            self.set_summary_fingerprint(None);
            self.publish_saved_focus();
        }
        Ok(None)
    }

    pub(super) async fn configure_engine(
        &mut self,
        profile: Option<EngineProfile>,
    ) -> Result<(), AppError> {
        self.ensure_no_benchmark()?;
        self.auto_setup.cancel();
        self.cancel_trial().await;
        // Validate argument conflicts before changing the saved configuration.
        if let Some(profile) = &profile {
            profile.transport()?;
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
        Ok(())
    }
}

#[cfg(test)]
mod tests;
