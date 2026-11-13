use super::Actor;
use crate::{
    AppError, AssetDownloadStatus, EngineEntry, EngineProfile, Settings, assets::AssetHandle,
};
use std::future::pending;
use tokio::sync::{oneshot, watch};

#[derive(Default)]
pub(super) struct State {
    pub(super) handle: Option<AssetHandle>,
    progress: Option<watch::Receiver<AssetDownloadStatus>>,
}

impl State {
    pub(super) async fn shutdown(&mut self) -> Result<(), AppError> {
        let result = if let Some(handle) = self.handle.take() {
            handle.shutdown().await.map_err(AppError::from)
        } else {
            Ok(())
        };
        self.progress = None;
        result
    }
}

pub(super) async fn receive(state: &mut State) -> AssetDownloadStatus {
    let Some(receiver) = &mut state.progress else {
        return pending().await;
    };
    if receiver.changed().await.is_err() {
        return pending().await;
    }
    receiver.borrow_and_update().clone()
}

impl Actor {
    pub(super) fn asset_handle(&mut self) -> Result<AssetHandle, AppError> {
        if let Some(handle) = &self.assets.handle {
            return Ok(handle.clone());
        }
        let directory = self
            .config
            .cache_directory
            .as_ref()
            .unwrap_or(&self.directory);
        #[cfg(not(test))]
        let (handle, progress) = crate::assets::spawn(directory.clone())?;
        #[cfg(test)]
        let (handle, progress) = if let Some(catalog) = &self.config.asset_catalog {
            crate::assets::spawn_with_catalog(directory.clone(), catalog.clone())
        } else {
            crate::assets::spawn(directory.clone())?
        };
        self.assets = State {
            handle: Some(handle.clone()),
            progress: Some(progress),
        };
        Ok(handle)
    }

    pub(super) async fn replace_main_engine(
        &mut self,
        expected: EngineEntry,
        profile: EngineProfile,
        restart: bool,
        reply: oneshot::Sender<Result<Settings, AppError>>,
    ) {
        // File verification/preparation can be slow. Never overwrite an engine
        // selection or edit that happened while that work was in flight.
        if crate::weights::selected_entry(&self.settings).ok().as_ref() != Some(&expected) {
            let _ = reply.send(Err(crate::engines::invalid(
                "main engine changed during preparation",
            )
            .into()));
            return;
        }
        let request = if restart && self.settings.engine.is_some() && profile == expected.profile {
            crate::engines::Request::Restart
        } else {
            crate::engines::Request::Update(expected.id, expected.name, profile)
        };
        self.engine_command(request, reply).await;
    }

    pub(super) async fn runtime_environment(
        &mut self,
        profile: &EngineProfile,
    ) -> Result<std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>, AppError> {
        if !crate::runtime::enabled(&self.config) {
            return Ok(Default::default());
        }
        match self
            .asset_handle()?
            .runtime_for_program(profile.program.clone())
            .await?
        {
            Some(installation) => crate::runtime::environment(&installation),
            None => Ok(Default::default()),
        }
    }

    pub(super) async fn with_runtime_environment(
        &mut self,
        profile: &EngineProfile,
        transport: &mut engine_session::TransportSpec,
    ) -> Result<(), AppError> {
        let engine_session::TransportSpec::LocalProcess { env, .. } = transport;
        env.extend(self.runtime_environment(profile).await?);
        Ok(())
    }
}
