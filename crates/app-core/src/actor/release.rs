use super::Actor;
use crate::{
    AppError,
    updates::{UpdateDownloadStatus, UpdateHandle},
};
use tokio::sync::watch;

#[derive(Default)]
pub(super) struct State {
    pub handle: Option<UpdateHandle>,
    progress: Option<watch::Receiver<UpdateDownloadStatus>>,
    pub install_pending: bool,
}

impl State {
    pub async fn shutdown(&mut self) -> Result<(), AppError> {
        self.progress = None;
        if let Some(handle) = self.handle.take() {
            handle.shutdown().await?;
        }
        Ok(())
    }
}

pub(super) async fn receive(state: &mut State) -> UpdateDownloadStatus {
    let Some(progress) = &mut state.progress else {
        return std::future::pending().await;
    };
    if progress.changed().await.is_err() {
        return std::future::pending().await;
    }
    progress.borrow_and_update().clone()
}

impl Actor {
    pub(super) async fn prepare_update_install(&mut self) -> Result<(), AppError> {
        if self.updates.install_pending {
            return Err(crate::updates::UpdateError::Busy.into());
        }
        if !self
            .updates
            .handle
            .as_ref()
            .is_some_and(|handle| handle.status().phase == crate::updates::UpdatePhase::Downloaded)
        {
            return Err(crate::updates::UpdateError::NotDownloaded.into());
        }
        self.updates.install_pending = true;
        let result = self.prepare_update_resources().await;
        if result.is_err() {
            self.recover_update_install().await;
        }
        result
    }

    async fn prepare_update_resources(&mut self) -> Result<(), AppError> {
        if self.batch_active() {
            self.stop_batch(Some("application update is installing".into()))
                .await;
        }
        self.shutdown_match().await;
        self.shutdown_gtp().await;
        self.auto_setup.cancel();
        self.cancel_trial().await;
        if let Some(assets) = &self.assets.handle {
            assets.cancel().await?;
        }
        // The ordinary cancellation path updates the terminal benchmark state;
        // simply reaping its process would leave the application permanently Busy.
        self.benchmark_command(crate::benchmark::Request::Cancel)
            .await?;
        if self.benchmark_active() {
            return Err(crate::BenchmarkError::Failed(
                "benchmark process could not be stopped for update".into(),
            )
            .into());
        }
        self.stop_board_sync().await?;
        self.stop_engine().await?;
        if let Some(cache) = &self.cache {
            cache.flush().await;
        }
        if let Some(support) = &self.support {
            support.flush().await?;
        }
        Ok(())
    }

    pub(super) async fn recover_update_install(&mut self) {
        if !std::mem::take(&mut self.updates.install_pending) {
            return;
        }
        if !self.benchmark_active() && self.running.is_none() {
            self.start_engine().await;
        }
        if let Some(handle) = &self.updates.handle {
            self.output
                .send(crate::AppEvent::UpdateDownloadProgress(handle.status()));
        }
    }

    pub(super) async fn update_progress(&mut self, progress: UpdateDownloadStatus) {
        if self.updates.install_pending
            && matches!(
                progress.phase,
                crate::updates::UpdatePhase::Failed | crate::updates::UpdatePhase::Cancelled
            )
        {
            // Only the owning install task may recover its lease. Defer its
            // failure event until recovery has finished, so a stale completion
            // can never release a subsequent installation's lease.
            return;
        }
        self.output
            .send(crate::AppEvent::UpdateDownloadProgress(progress));
    }

    /// Freeze mutations between resource preparation and installer completion.
    /// Reads and shutdown remain available; failed installation releases the
    /// lease before restoring the saved live-analysis intent.
    pub(super) fn update_guard(&self, command: super::Command) -> Option<super::Command> {
        use super::{Command, Input};
        if !self.updates.install_pending {
            return Some(command);
        }
        macro_rules! reject {
            ($reply:expr) => {{
                let _ = $reply.send(Err(crate::updates::UpdateError::Busy.into()));
                None
            }};
        }
        match command {
            command @ Command::Apply(Input::Shutdown | Input::RecoverUpdateInstall, _) => {
                Some(command)
            }
            Command::Apply(_, reply) => reject!(reply),
            Command::Updates(reply) => reject!(reply),
            Command::Assets(reply) => reject!(reply),
            Command::AttachUpdates(_, reply) => reject!(reply),
            Command::ReplaceMainEngine(_, _, _, reply) | Command::Engines(_, reply) => {
                reject!(reply)
            }
            Command::SetEngineSettings(_, reply) => reject!(reply),
            Command::TryConfigureEngine(_, _, reply) => reject!(reply),
            Command::TryAutoEngine(_, reply) => reject!(reply),
            Command::BeginAutoSetup(reply) => reject!(reply),
            Command::Benchmark(crate::benchmark::Request::Status, _) => Some(command),
            Command::Benchmark(_, reply) => reject!(reply),
            Command::BoardSync(crate::board_sync::Request::Status, _) => Some(command),
            Command::BoardSync(_, reply) => reject!(reply),
            Command::Game(_, reply) => reject!(reply),
            Command::Match(_, reply) => reject!(reply),
            Command::Gtp(request) => {
                use crate::gtp_engines::Request;
                match request {
                    Request::Add(_, reply)
                    | Request::Update(_, reply)
                    | Request::Remove(_, reply) => reject!(reply),
                    Request::Test(_, reply) => reject!(reply),
                    Request::Send(_, _, reply) => reject!(reply),
                    request @ Request::Console(_, _) => Some(Command::Gtp(request)),
                }
            }
            Command::SetAiPlayer(_, _, reply) => reject!(reply),
            Command::PlayBestMove(_, reply) => reject!(reply),
            Command::Review(_, reply) => reject!(reply),
            Command::Batch(_, reply) => reject!(reply),
            Command::SetDisplaySettings(_, reply) => reject!(reply),
            Command::SetMoveRankSettings(_, reply) => reject!(reply),
            Command::ClaimUpdateCheck(_, reply) => reject!(reply),
            Command::AnalysisLimit(crate::analysis_limit::Request::Set(_, reply)) => reject!(reply),
            Command::AnalysisLimit(crate::analysis_limit::Request::Toggle(reply)) => reject!(reply),
            Command::JavaImport(crate::java_import::Request::Confirm(_, reply)) => reject!(reply),
            Command::Fox(
                crate::fox::Request::Remember(_, reply) | crate::fox::Request::Clear(reply),
            ) => reject!(reply),
            Command::Tencent(
                crate::tencent::Request::Remember(_, reply) | crate::tencent::Request::Clear(reply),
            ) => reject!(reply),
            command => Some(command),
        }
    }

    pub(super) fn attach_updates(&mut self, handle: UpdateHandle) -> Result<(), AppError> {
        if self.updates.handle.is_some() {
            return Err(crate::updates::UpdateError::Busy.into());
        }
        self.output
            .send(crate::AppEvent::UpdateDownloadProgress(handle.status()));
        self.updates.progress = Some(handle.subscribe_status());
        self.updates.handle = Some(handle);
        Ok(())
    }
}
