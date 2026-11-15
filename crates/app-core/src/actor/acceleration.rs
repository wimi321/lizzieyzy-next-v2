use super::{Actor, Command, Input, trial};
use crate::{
    AccelerationBackend, AccelerationError, AccelerationPhase, AccelerationProgress, AppError,
    AppEvent, AssetDownloadStatus, DownloadPhase, EngineEntry, Settings, assets::AssetHandle,
};
use std::future::pending;
use tokio::sync::{oneshot, watch};

#[derive(Default)]
pub(super) struct State {
    pub backend: Option<AccelerationBackend>,
    download: Option<Download>,
    pub output: Vec<String>,
    pub backend_line: Option<String>,
    // Keep the temporary logger alive until its adopted process is reaped.
    pub log: Option<crate::acceleration::log::Observation>,
}

struct Download {
    expected: EngineEntry,
    handle: AssetHandle,
    status: watch::Receiver<AssetDownloadStatus>,
    reply: oneshot::Sender<Result<Settings, AppError>>,
}

pub(super) enum Update {
    Downloaded,
    Failed(AppError),
}

pub(super) async fn receive(state: &mut State) -> Update {
    let Some(download) = &mut state.download else {
        return pending().await;
    };
    let Some(backend) = state.backend else {
        return pending().await;
    };
    loop {
        let status = download.status.borrow_and_update().clone();
        if status.asset_id.as_deref() != Some(backend.profile_id()) {
            return Update::Failed(failure("runtime installation was replaced"));
        }
        match status.phase {
            DownloadPhase::Completed => return Update::Downloaded,
            DownloadPhase::Cancelled => return Update::Failed(cancelled()),
            DownloadPhase::Failed => {
                return Update::Failed(failure(
                    status
                        .error
                        .as_deref()
                        .unwrap_or("runtime installation failed"),
                ));
            }
            _ => {}
        }
        tokio::select! {
            _ = download.reply.closed() => return Update::Failed(cancelled()),
            changed = download.status.changed() => {
                if changed.is_err() { return Update::Failed(failure("asset downloader closed")); }
            }
        }
    }
}

fn failure(reason: &str) -> AppError {
    AccelerationError::Failed {
        reason: reason.into(),
        stderr_tail: Vec::new(),
    }
    .into()
}

fn cancelled() -> AppError {
    AccelerationError::Cancelled {
        stderr_tail: Vec::new(),
    }
    .into()
}

impl Actor {
    pub(super) fn ensure_no_acceleration(&self) -> Result<(), AppError> {
        if self.acceleration.backend.is_some() {
            Err(AccelerationError::Busy.into())
        } else {
            Ok(())
        }
    }

    pub(super) fn publish_acceleration(&self, phase: AccelerationPhase) {
        if let Some(backend) = self.acceleration.backend {
            self.output
                .send(AppEvent::AccelerationProgress(AccelerationProgress {
                    backend,
                    phase,
                    output_tail: self.acceleration.output.clone(),
                }));
        }
    }

    pub(super) async fn begin_acceleration(
        &mut self,
        backend: AccelerationBackend,
        reply: oneshot::Sender<Result<Settings, AppError>>,
    ) {
        let prepared = async {
            self.ensure_no_acceleration()?;
            if self.benchmark_active() || self.trial.is_some() {
                return Err(AccelerationError::Busy.into());
            }
            if !crate::runtime::enabled(&self.config) {
                return Err(AccelerationError::UnsupportedPlatform.into());
            }
            self.ensure_no_batch()?;
            if self.game_playing() || self.coach.is_starting() || self.match_active() {
                return Err(AccelerationError::Busy.into());
            }
            let expected = crate::weights::selected_entry(&self.settings)
                .map_err(|_| AccelerationError::NoEngine)?;
            if expected.profile.remote.is_some() {
                return Err(AccelerationError::NoEngine.into());
            }
            let handle = self.asset_handle()?;
            Ok((expected, handle))
        }
        .await;
        let (expected, handle) = match prepared {
            Ok(value) => value,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        self.auto_setup.cancel();
        self.acceleration.backend = Some(backend);
        self.acceleration.output.clear();
        self.acceleration.backend_line = None;
        self.publish_acceleration(AccelerationPhase::Downloading);
        let status = handle.subscribe_status();
        if let Err(error) = handle.install_runtime(backend.profile_id().into()).await {
            let error = self.acceleration_failed(error.into(), true).await;
            let _ = reply.send(Err(error));
            return;
        }
        self.acceleration.download = Some(Download {
            expected,
            handle,
            status,
            reply,
        });
    }

    pub(super) async fn acceleration_download(&mut self, update: Update) {
        let Some(download) = self.acceleration.download.take() else {
            return;
        };
        let prepared = async {
            if let Update::Failed(error) = update {
                return Err(error);
            }
            if download.reply.is_closed() {
                return Err(cancelled());
            }
            if crate::weights::selected_entry(&self.settings).ok().as_ref()
                != Some(&download.expected)
            {
                return Err(failure(
                    "main engine changed during acceleration preparation",
                ));
            }
            let backend = self.acceleration.backend.ok_or(AccelerationError::Busy)?;
            let installation = download
                .handle
                .runtime_installation(backend.profile_id().into())
                .await?;
            if !installation.ready {
                return Err(failure("runtime failed its final integrity check"));
            }
            let mut profile = download.expected.profile.clone();
            profile.program = installation
                .program
                .ok_or_else(|| failure("runtime has no executable"))?;
            let mut entries = self.settings.engines.clone();
            let entry = entries
                .iter_mut()
                .find(|entry| entry.id == download.expected.id)
                .ok_or(AccelerationError::NoEngine)?;
            entry.profile = profile.clone();
            entry.name = match backend {
                AccelerationBackend::Cuda => "KataGo CUDA",
                AccelerationBackend::TensorRt => "KataGo TensorRT",
                AccelerationBackend::OpenCl => "KataGo OpenCL",
            }
            .into();
            Ok((entries, download.expected.id.clone(), profile))
        }
        .await;
        match prepared {
            Ok((entries, id, profile)) => {
                // Never start another process when cleanup of the old owner
                // failed: a second empty stop cannot prove it was reaped.
                if let Err(error) = self.stop_engine().await {
                    let error = failure(&format!(
                        "previous engine cleanup failed; acceleration was not started: {error}"
                    ));
                    let error = self.acceleration_failed(error, false).await;
                    let _ = download.reply.send(Err(error));
                    return;
                }
                self.publish_acceleration(AccelerationPhase::Starting);
                self.begin_list_trial(
                    entries,
                    id,
                    profile,
                    trial::Reply::Acceleration(download.reply),
                )
                .await;
            }
            Err(error) => {
                let _ = download.handle.cancel().await;
                let error = self.acceleration_failed(error, true).await;
                let _ = download.reply.send(Err(error));
            }
        }
    }

    pub(super) async fn acceleration_failed(&mut self, error: AppError, restart: bool) -> AppError {
        let mut tail = crate::acceleration::output::OutputTail::default();
        for line in &self.acceleration.output {
            tail.push(line);
        }
        if let AppError::EngineSetup(crate::EngineSetupError::Failed { stderr_tail, .. }) = &error {
            for line in stderr_tail {
                tail.push(line);
            }
        }
        self.acceleration.output = tail.lines();
        let stderr_tail = self.acceleration.output.clone();
        let error = match error {
            AppError::EngineSetup(crate::EngineSetupError::Cancelled)
            | AppError::Acceleration(AccelerationError::Cancelled { .. }) => {
                AccelerationError::Cancelled { stderr_tail }
            }
            AppError::EngineSetup(crate::EngineSetupError::TimedOut(_)) => {
                AccelerationError::TimedOut { stderr_tail }
            }
            AppError::EngineSetup(crate::EngineSetupError::Failed { reason, .. })
            | AppError::Acceleration(AccelerationError::Failed { reason, .. }) => {
                AccelerationError::Failed {
                    reason,
                    stderr_tail,
                }
            }
            error => AccelerationError::Failed {
                reason: error.to_string(),
                stderr_tail,
            },
        };
        if restart {
            let cleanup = self.stop_engine().await;
            if cleanup.is_ok() {
                Box::pin(self.start_engine()).await;
            } else if let Err(error) = cleanup {
                self.analysis_failed(error.to_string());
            }
        }
        self.publish_acceleration(AccelerationPhase::Failed);
        self.acceleration.backend = None;
        error.into()
    }

    pub(super) async fn cancel_acceleration(&mut self, restart: bool) {
        if self.acceleration.backend.is_none() {
            return;
        }
        if let Some(download) = self.acceleration.download.take() {
            let _ = download.handle.cancel().await;
            let error = self.acceleration_failed(cancelled(), restart).await;
            let _ = download.reply.send(Err(error));
        } else {
            // cancel_trial maps the reply while this marker is still present.
            self.cancel_acceleration_trial(restart).await;
        }
    }

    pub(super) async fn cancel_asset_download(&mut self) -> Result<AssetDownloadStatus, AppError> {
        // Keep cancellation in one actor command. Looking up the asset handle
        // after this acknowledgement could cancel a newly queued activation.
        let handle = self.asset_handle()?;
        if self.acceleration.backend.is_some() {
            self.cancel_acceleration(true).await;
            Ok(handle.status())
        } else {
            Ok(handle.cancel().await?)
        }
    }

    pub(super) async fn dismiss_acceleration_hint(&mut self) -> Result<(), AppError> {
        let mut settings = self.settings.clone();
        settings.acceleration_hint_dismissed = true;
        settings.save(&self.directory).await?;
        self.settings = settings;
        Ok(())
    }

    /// Reserve engine mutation/benchmark entry points across download awaits.
    pub(super) fn acceleration_guard(&self, command: Command) -> Option<Command> {
        if self.acceleration.backend.is_none() {
            return Some(command);
        }
        macro_rules! reject {
            ($reply:expr) => {{
                let _ = $reply.send(Err(AccelerationError::Busy.into()));
                None
            }};
        }
        match command {
            Command::EnableAcceleration(_, reply)
            | Command::Engines(_, reply)
            | Command::ReplaceMainEngine(_, _, _, reply) => reject!(reply),
            Command::TryConfigureEngine(_, _, reply) => reject!(reply),
            Command::TryAutoEngine(_, reply) => reject!(reply),
            Command::BeginAutoSetup(reply) => reject!(reply),
            Command::SetEngineSettings(_, reply) => reject!(reply),
            Command::Apply(Input::ConfigureEngine(_) | Input::PrepareUpdateInstall, reply) => {
                reject!(reply)
            }
            Command::Benchmark(
                crate::benchmark::Request::Start | crate::benchmark::Request::Reset,
                reply,
            ) => reject!(reply),
            Command::Remote(
                crate::remote::Request::Add(_, _, reply)
                | crate::remote::Request::Update(_, _, _, reply)
                | crate::remote::Request::Local(reply),
            ) => reject!(reply),
            Command::JavaImport(crate::java_import::Request::Confirm(_, reply)) => reject!(reply),
            Command::Coach(crate::coach_api::Request::Start(_, reply)) => reject!(reply),
            Command::Game(_, reply) => reject!(reply),
            Command::Match(_, reply) => reject!(reply),
            Command::Batch(_, reply) => reject!(reply),
            command => Some(command),
        }
    }
}

#[cfg(test)]
mod tests;
