use super::{Actor, trial};
use crate::{
    AppError, AppEvent, EngineProfile, EngineStatus, RemoteEngineError,
    remote::{RemoteSecrets, Request},
};
use engine_session::TransportSpec;
use std::path::PathBuf;

impl Actor {
    fn known_hosts(&self) -> PathBuf {
        self.config
            .cache_directory
            .as_ref()
            .unwrap_or(&self.directory)
            .join("ssh/known_hosts")
    }

    pub(super) async fn prepare_engine_transport(
        &mut self,
        profile: &EngineProfile,
    ) -> Result<(TransportSpec, String), AppError> {
        let fingerprint = profile.fingerprint().await?;
        let known_hosts = self.known_hosts();
        let mut transport = match &profile.remote {
            Some(remote) => self.remote.transport(remote, known_hosts).await?,
            None => profile.transport_with_benchmark(self.settings.benchmarks.get(&fingerprint))?,
        };
        self.with_runtime_environment(profile, &mut transport)
            .await?;
        Ok((transport, fingerprint))
    }

    pub(super) async fn remote_command(&mut self, request: Request) {
        // These inputs carry credentials, so never derive/log Debug for Request.
        match request {
            Request::Add(name, input, reply) => {
                if let Err(error) = self.ensure_no_benchmark().and_then(|()| valid_name(&name)) {
                    let _ = reply.send(Err(error));
                    return;
                }
                self.cancel_trial().await;
                match self.remote.import(input).await {
                    Ok(profile) => {
                        let saved = self
                            .engine_command(crate::engines::Request::Add(name, profile), reply)
                            .await;
                        self.finish_remote_edit(saved).await;
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
            }
            Request::Update(id, name, input, reply) => {
                if let Err(error) = self.ensure_no_benchmark().and_then(|()| valid_name(&name)) {
                    let _ = reply.send(Err(error));
                    return;
                }
                self.cancel_trial().await;
                if !self.settings.engines.iter().any(|entry| entry.id == id) {
                    let _ = reply.send(Err(
                        crate::engines::invalid("engine ID does not exist").into()
                    ));
                    return;
                }
                match self.remote.import(input).await {
                    Ok(profile) => {
                        let saved = self
                            .engine_command(
                                crate::engines::Request::Update(id, name, profile),
                                reply,
                            )
                            .await;
                        self.finish_remote_edit(saved).await;
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
            }
            Request::Prepare(input, reply) => {
                let result = RemoteSecrets::preview(input, self.known_hosts())
                    .map(|transport| (transport, self.session_config()));
                let _ = reply.send(result);
            }
            Request::Local(reply) => {
                let mut selected = None;
                for entry in &self.settings.engines {
                    if entry.profile.remote.is_none()
                        && entry.profile.fingerprint().await.is_ok()
                        && entry.profile.transport().is_ok()
                    {
                        selected = Some(entry.id.clone());
                        break;
                    }
                }
                match selected {
                    Some(id) => {
                        self.engine_command(crate::engines::Request::Select(id), reply)
                            .await;
                    }
                    None => {
                        let _ = reply.send(Err(RemoteEngineError::NoLocalEngine.into()));
                    }
                }
            }
        }
    }

    async fn finish_remote_edit(&mut self, saved: bool) {
        if self.trial.is_none() {
            if saved {
                self.remote.commit().await;
            } else {
                self.remote.rollback();
            }
        }
    }

    pub(super) async fn start_remote_engine(&mut self, profile: EngineProfile) {
        let Some(id) = self.settings.main_engine_id.clone() else {
            self.failed(RemoteEngineError::InvalidConfiguration.to_string());
            return;
        };
        self.output
            .send(AppEvent::EngineStatusChanged(EngineStatus::Starting));
        self.begin_list_trial(
            self.settings.engines.clone(),
            id,
            profile,
            trial::Reply::Startup,
        )
        .await;
    }
}

fn valid_name(name: &str) -> Result<(), AppError> {
    if name.trim().is_empty() {
        Err(crate::engines::invalid("engine name must be nonempty").into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
