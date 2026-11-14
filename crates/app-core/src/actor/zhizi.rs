use super::{Actor, Running};
use crate::{
    AppError, AppEvent, EngineProfile, EngineStatus, RemoteEngineConfig, RemoteEngineError,
    ZhiziEngineOptions,
};
use engine_session::{RemoteTransportError, TransportSpec, ZhiziConnection, ZhiziConnector};

impl Actor {
    pub(super) async fn zhizi_credentials_changed(&mut self) -> Result<(), AppError> {
        if self
            .trial
            .as_ref()
            .is_some_and(super::trial::PendingTrial::is_zhizi)
        {
            self.cancel_trial().await;
        }
        let active_zhizi = !self.startup_local_fallback
            && self.running.is_some()
            && self.settings.engine.as_ref().is_some_and(|profile| {
                matches!(profile.remote, Some(RemoteEngineConfig::Zhizi { .. }))
            });
        if active_zhizi {
            // A session token can outlive its login token. Stop the former
            // account's compute immediately, without changing saved choices.
            self.stop_engine().await?;
        }
        Ok(())
    }

    pub(super) async fn start_initial_engine(&mut self) {
        let selected_zhizi = self.settings.engine.as_ref().is_some_and(|profile| {
            matches!(profile.remote, Some(RemoteEngineConfig::Zhizi { .. }))
        });
        if selected_zhizi {
            // The service bounds keychain access; failure is an unauthenticated
            // session, never an application startup failure.
            let _ = self.zhizi.restore().await;
        } else {
            let service = self.zhizi.clone();
            tokio::spawn(async move {
                let _ = service.restore().await;
            });
        }
        let needs_local = selected_zhizi && !self.zhizi.status().logged_in;
        if needs_local {
            for entry in &self.settings.engines {
                if entry.profile.remote.is_none()
                    && entry.profile.fingerprint().await.is_ok()
                    && entry.profile.transport().is_ok()
                {
                    let profile = entry.profile.clone();
                    self.startup_local_fallback = true;
                    // The persisted main selection stays Zhizi. No settings
                    // projection is changed, even if another preference saves.
                    self.start_engine_profile(profile).await;
                    return;
                }
            }
        }
        self.start_engine().await;
    }

    pub(super) fn zhizi_transport(
        &self,
        options: ZhiziEngineOptions,
    ) -> Result<TransportSpec, AppError> {
        if !self.zhizi.status().logged_in {
            return Err(RemoteEngineError::ZhiziLoginRequired.into());
        }
        let service = self.zhizi.clone();
        let args = options.connection_args();
        Ok(TransportSpec::Zhizi {
            connect: ZhiziConnector::new(move || {
                let service = service.clone();
                let args = args.clone();
                async move {
                    let token = service.socket_token(args).await.map_err(connection_error)?;
                    Ok(ZhiziConnection::new(token.url, token.token))
                }
            }),
        })
    }
    pub(super) async fn start_engine(&mut self) {
        self.startup_local_fallback = false;
        let Some(profile) = self.settings.engine.clone() else {
            self.set_summary_fingerprint(None);
            self.output
                .send(AppEvent::EngineStatusChanged(EngineStatus::Stopped));
            self.publish_saved_focus();
            return;
        };
        self.start_engine_profile(profile).await;
    }

    async fn start_engine_profile(&mut self, profile: EngineProfile) {
        if profile.remote.is_some() {
            self.start_remote_engine(profile).await;
            return;
        }
        let prepared = self.prepare_engine_transport(&profile).await;
        let (transport, fingerprint) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.set_summary_fingerprint(None);
                self.failed(error.to_string());
                self.publish_saved_focus();
                return;
            }
        };
        self.set_summary_fingerprint(Some(fingerprint.clone()));
        let engine = engine_session::spawn(transport, self.session_config());
        match self.coordinator(&engine, &fingerprint) {
            Ok((coordinator, snapshots)) => {
                let engine_status = engine.status();
                self.output.send(AppEvent::EngineStatusChanged(
                    engine_status.borrow().clone().into(),
                ));
                self.running = Some(Running {
                    engine,
                    analysis_status: coordinator.status(),
                    coordinator,
                    snapshots,
                    engine_status,
                    fingerprint,
                });
                self.update_focus().await;
            }
            Err(error) => {
                let _ = engine.shutdown().await;
                self.failed(error.to_string());
            }
        }
    }
}

fn connection_error(error: crate::zhizi::ZhiziError) -> RemoteTransportError {
    use crate::zhizi::ZhiziErrorKind as K;
    match error.kind {
        K::InsufficientBalance => RemoteTransportError::ZhiziBalance,
        K::VipExpired => RemoteTransportError::ZhiziVip,
        K::LoginRequired | K::LoginExpired => RemoteTransportError::ZhiziAccessDenied,
        K::UnsafeAddress => RemoteTransportError::InvalidConfiguration,
        _ if matches!(error.status, 401 | 403) => RemoteTransportError::ZhiziAccessDenied,
        _ => RemoteTransportError::Connect,
    }
}

#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod tests;
