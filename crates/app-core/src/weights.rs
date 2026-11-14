//! Download and diagnostics commands. Long I/O runs outside the document actor;
//! only the guarded profile replacement enters the existing engine transaction.
use crate::{
    AccelerationPlan, AppCore, AppError, AssetDownloadStatus, DiagnosticRepair, EngineDiagnostics,
    EngineEntry, EngineProfile, OfficialWeight, RepairResources, Settings, actor::Command,
};
use tokio::sync::oneshot;

impl AppCore {
    async fn asset_handle(&self) -> Result<crate::assets::AssetHandle, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Assets(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    pub async fn official_weights(&self) -> Result<Vec<OfficialWeight>, AppError> {
        Ok(self.asset_handle().await?.list_weights().await?)
    }

    pub async fn start_asset_download(
        &self,
        asset_id: String,
    ) -> Result<AssetDownloadStatus, AppError> {
        Ok(self.asset_handle().await?.start(asset_id).await?)
    }

    pub async fn pause_asset_download(&self) -> Result<AssetDownloadStatus, AppError> {
        Ok(self.asset_handle().await?.pause().await?)
    }

    pub async fn humansl_model_status(&self) -> Result<crate::HumanSlModelStatus, AppError> {
        Ok(self.asset_handle().await?.humansl_status().await?)
    }

    pub async fn import_humansl_model(
        &self,
        path: std::path::PathBuf,
    ) -> Result<crate::HumanSlModelStatus, AppError> {
        Ok(self.asset_handle().await?.import_humansl(path).await?)
    }

    pub async fn cancel_asset_download(&self) -> Result<AssetDownloadStatus, AppError> {
        Ok(self.asset_handle().await?.cancel().await?)
    }

    pub async fn asset_download_status(&self) -> Result<AssetDownloadStatus, AppError> {
        Ok(self.asset_handle().await?.status())
    }

    pub async fn acceleration_plan(&self) -> Result<AccelerationPlan, AppError> {
        Ok(self.asset_handle().await?.acceleration_plan().await?)
    }

    pub async fn use_official_weight(&self, asset_id: String) -> Result<Settings, AppError> {
        let entry = selected_entry(&self.settings().await?)?;
        let path = self.asset_handle().await?.verified_weight(asset_id).await?;
        let mut profile = entry.profile.clone();
        profile.canonical_model_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        profile.model = path;
        self.replace_main_profile(entry, profile).await
    }

    pub async fn diagnose_engine(
        &self,
        resources: RepairResources,
    ) -> Result<EngineDiagnostics, AppError> {
        let settings = self.settings().await?;
        let profile = selected_entry(&settings).ok().map(|entry| entry.profile);
        if !self.runtime_supported() || profile.is_none() {
            return crate::diagnostics::diagnose(profile, resources).await;
        }
        let runtime = self
            .asset_handle()
            .await?
            .runtime_for_program(
                profile
                    .as_ref()
                    .map(|value| value.program.clone())
                    .unwrap_or_default(),
            )
            .await?;
        crate::diagnostics::diagnose_with_runtime(profile, resources, runtime).await
    }

    pub async fn repair_engine(
        &self,
        repair: DiagnosticRepair,
        resources: RepairResources,
    ) -> Result<EngineDiagnostics, AppError> {
        let entry = selected_entry(&self.settings().await?)?;
        if repair == DiagnosticRepair::InstallRuntime {
            if !self.runtime_supported() {
                return Err(crate::engines::invalid(
                    "NVIDIA runtime installation is only needed on Windows x64",
                )
                .into());
            }
            let handle = self.asset_handle().await?;
            let runtime = handle
                .runtime_for_program(entry.profile.program.clone())
                .await?
                .ok_or_else(|| {
                    crate::engines::invalid(
                        "selected engine does not require a managed NVIDIA runtime",
                    )
                })?;
            let mut progress = handle.subscribe_status();
            handle.install_runtime(runtime.profile_id.clone()).await?;
            loop {
                let status = progress.borrow_and_update().clone();
                if status.asset_id.as_ref() != Some(&runtime.profile_id) {
                    return Err(crate::engines::invalid("runtime installation was replaced").into());
                }
                match status.phase {
                    crate::DownloadPhase::Completed => break,
                    crate::DownloadPhase::Failed | crate::DownloadPhase::Cancelled => {
                        return Err(crate::engines::invalid(
                            status
                                .error
                                .as_deref()
                                .unwrap_or("runtime installation was cancelled"),
                        )
                        .into());
                    }
                    _ => progress.changed().await.map_err(|_| AppError::Closed)?,
                }
            }
            let installed = handle.runtime_installation(runtime.profile_id).await?;
            let mut profile = entry.profile.clone();
            if !installed.ready {
                return Err(crate::engines::invalid(
                    "runtime installation failed its final integrity check",
                )
                .into());
            }
            // Use the same pinned engine from the completed generation. This
            // also repairs a damaged old generation without overwriting files
            // an existing Windows process may still have loaded.
            profile.program = installed.program.ok_or_else(|| {
                crate::engines::invalid("installed runtime has no engine executable")
            })?;
            self.apply_prepared_main(entry, profile, true).await?;
            return self.diagnose_engine(resources).await;
        }
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::DataDirectory(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        let directory = response.await.map_err(|_| AppError::Closed)?;
        let profile = crate::diagnostics::prepare_repair(
            entry.profile.clone(),
            repair,
            resources.clone(),
            directory,
        )
        .await?;
        self.replace_main_profile(entry, profile).await?;
        self.diagnose_engine(resources).await
    }

    async fn replace_main_profile(
        &self,
        expected: EngineEntry,
        profile: EngineProfile,
    ) -> Result<Settings, AppError> {
        self.apply_prepared_main(expected, profile, false).await
    }

    async fn apply_prepared_main(
        &self,
        expected: EngineEntry,
        profile: EngineProfile,
        restart: bool,
    ) -> Result<Settings, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::ReplaceMainEngine(
                expected, profile, restart, reply,
            ))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

pub(crate) fn selected_entry(settings: &Settings) -> Result<EngineEntry, AppError> {
    settings
        .engines
        .iter()
        .find(|entry| Some(&entry.id) == settings.main_engine_id.as_ref())
        .cloned()
        .ok_or_else(|| crate::engines::invalid("no main engine selected").into())
}

#[cfg(test)]
mod tests;
