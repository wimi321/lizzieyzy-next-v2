//! Release operations perform slow I/O outside the document actor.
use crate::{
    AppCore, AppError, FullLogStatus, RepairResources, UpdateCheck, UpdateDownloadStatus,
    actor::Command,
};
use tokio::sync::oneshot;

impl AppCore {
    pub async fn attach_updates(
        &self,
        handle: crate::updates::UpdateHandle,
    ) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::AttachUpdates(handle, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    async fn update_handle(&self) -> Result<crate::updates::UpdateHandle, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Updates(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    pub async fn support_handle(&self) -> Result<crate::support::SupportHandle, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Support(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    pub async fn check_update(&self) -> Result<UpdateCheck, AppError> {
        Ok(self.update_handle().await?.check().await?)
    }

    pub async fn download_update(&self) -> Result<UpdateDownloadStatus, AppError> {
        Ok(self.update_handle().await?.download().await?)
    }

    pub async fn cancel_update_download(&self) -> Result<UpdateDownloadStatus, AppError> {
        Ok(self.update_handle().await?.cancel().await?)
    }

    pub async fn update_download_status(&self) -> Result<UpdateDownloadStatus, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::UpdateStatus(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    pub async fn install_update(&self) -> Result<UpdateDownloadStatus, AppError> {
        // The operation owns its lease even if the IPC caller disconnects.
        // Spawn before preparation so cancellation cannot strand a prepared
        // core between acquiring the lease and dispatching the installer.
        let core = self.clone();
        tokio::spawn(async move { core.install_update_owned().await })
            .await
            .map_err(|error| crate::updates::UpdateError::Backend(error.to_string()))?
    }

    async fn install_update_owned(&self) -> Result<UpdateDownloadStatus, AppError> {
        let handle = self.update_handle().await?;
        if matches!(
            handle.status().phase,
            crate::updates::UpdatePhase::Installing
        ) {
            return Err(crate::updates::UpdateError::Busy.into());
        }
        if handle.status().phase != crate::updates::UpdatePhase::Downloaded {
            return Err(crate::updates::UpdateError::NotDownloaded.into());
        }
        // Windows' installer may terminate this process directly. Flush and
        // reap before entering it, without asking the updater to stop itself.
        // Preparation owns its rollback. A second caller denied the install
        // lease must never recover/restart resources owned by the first one.
        self.send(crate::actor::Input::PrepareUpdateInstall).await?;
        match handle.install().await {
            Ok(status) => Ok(status),
            Err(error) => {
                let _ = self.send(crate::actor::Input::RecoverUpdateInstall).await;
                Err(error.into())
            }
        }
    }

    pub async fn full_log_status(&self) -> Result<FullLogStatus, AppError> {
        Ok(self.support_handle().await?.full_log_status().await?)
    }

    pub async fn log_directory(&self) -> Result<std::path::PathBuf, AppError> {
        Ok(self.support_handle().await?.log_directory().await?)
    }

    pub async fn resolve_reveal_path(
        &self,
        path: std::path::PathBuf,
    ) -> Result<std::path::PathBuf, AppError> {
        Ok(self
            .support_handle()
            .await?
            .resolve_reveal_path(path)
            .await?)
    }

    pub async fn start_full_log(&self) -> Result<FullLogStatus, AppError> {
        Ok(self.support_handle().await?.start_full_log().await?)
    }

    pub async fn stop_full_log(&self) -> Result<FullLogStatus, AppError> {
        Ok(self.support_handle().await?.stop_full_log().await?)
    }

    pub async fn export_diagnostics(
        &self,
        resources: RepairResources,
        version: String,
    ) -> Result<std::path::PathBuf, AppError> {
        let support = self.support_handle().await?;
        let settings = self.settings().await?;
        let diagnostics = self.diagnose_engine(resources).await?;
        Ok(support
            .export_bundle(settings, diagnostics, version)
            .await?)
    }
}
