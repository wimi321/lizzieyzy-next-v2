use super::Actor;
use crate::{
    AppError, AppEvent, BenchmarkError, BenchmarkPhase, BenchmarkResult, BenchmarkStatus,
    WholeGameStatus,
    benchmark::{
        Request,
        process::{Process, Update},
    },
    review::ReviewInput,
};
use std::{
    future::pending,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
pub(super) struct State {
    process: Option<Process>,
    fingerprint: Option<String>,
    profile: Option<crate::EngineProfile>,
    status: BenchmarkStatus,
    resume_review: bool,
}

pub(super) async fn receive(state: &mut State) -> Result<Update, BenchmarkError> {
    if state.status.phase != BenchmarkPhase::Running {
        return pending().await;
    }
    match &mut state.process {
        Some(process) => process.next().await,
        None => pending().await,
    }
}

impl Actor {
    pub(super) fn benchmark_review_input(&mut self, input: &ReviewInput) {
        if self.benchmark_active() && matches!(input, ReviewInput::Pause | ReviewInput::Cancel) {
            self.benchmark.resume_review = false;
        }
    }

    pub(super) fn benchmark_active(&self) -> bool {
        self.benchmark.status.phase == BenchmarkPhase::Running || self.benchmark.process.is_some()
    }

    pub(super) fn ensure_no_benchmark(&self) -> Result<(), AppError> {
        if self.benchmark_active() {
            Err(BenchmarkError::Busy.into())
        } else {
            Ok(())
        }
    }

    pub(super) async fn benchmark_command(
        &mut self,
        request: Request,
    ) -> Result<BenchmarkStatus, AppError> {
        match request {
            Request::Start => self.begin_benchmark().await?,
            Request::Cancel => {
                if self.benchmark_active() {
                    self.finish_benchmark(None, BenchmarkPhase::Cancelled).await;
                }
            }
            Request::Status => {}
            Request::Reset => self.reset_benchmark().await?,
        }
        self.benchmark_status().await
    }

    async fn benchmark_status(&self) -> Result<BenchmarkStatus, AppError> {
        let fingerprint = match &self.settings.engine {
            Some(profile) => profile.fingerprint().await.ok(),
            None => None,
        };
        let same_attempt = self.settings.engine == self.benchmark.profile
            && (fingerprint == self.benchmark.fingerprint
                || self.benchmark.status.phase == BenchmarkPhase::Failed);
        let mut status = if self.benchmark_active() || same_attempt {
            self.benchmark.status.clone()
        } else {
            BenchmarkStatus::default()
        };
        status.last_result =
            fingerprint.and_then(|key| self.settings.benchmarks.get(&key).cloned());
        Ok(status)
    }

    async fn publish_benchmark(&self) {
        if let Ok(status) = self.benchmark_status().await {
            self.output.send(AppEvent::BenchmarkProgress(status));
        }
    }

    async fn begin_benchmark(&mut self) -> Result<(), AppError> {
        self.ensure_no_benchmark()?;
        let profile = self
            .settings
            .engine
            .clone()
            .ok_or(BenchmarkError::NoEngine)?;
        profile.transport()?;
        let fingerprint = profile.fingerprint().await?;
        self.auto_setup.cancel();
        self.cancel_trial().await;
        self.benchmark.fingerprint = Some(fingerprint);
        self.benchmark.profile = Some(profile.clone());
        self.benchmark.status = BenchmarkStatus {
            phase: BenchmarkPhase::Running,
            ..Default::default()
        };
        // Do not persist a temporary pondering=false or lose the captured review line.
        self.benchmark.resume_review = self.review_running();
        let stopped = self.suspend_benchmark_analysis().await;
        if let Err(error) = stopped {
            self.finish_benchmark(Some(error.to_string()), BenchmarkPhase::Failed)
                .await;
            return Err(error);
        }
        match Process::spawn(&profile).await {
            Ok(process) => {
                self.benchmark.process = Some(process);
                self.publish_benchmark().await;
                Ok(())
            }
            Err(error) => {
                self.finish_benchmark(Some(error.to_string()), BenchmarkPhase::Failed)
                    .await;
                Err(error.into())
            }
        }
    }

    /// Waiting for the actual session process to exit is stronger than a
    /// terminate acknowledgement, which may precede KataGo's last search result.
    async fn suspend_benchmark_analysis(&mut self) -> Result<(), AppError> {
        if self.review_running() {
            self.review_command(ReviewInput::Pause).await?;
        }
        self.output.clear_analysis();
        self.flush_cached_analysis().await;
        self.focus = None;
        if let Some(running) = self.running.take() {
            let coordinator = running.coordinator.shutdown().await;
            let engine = running.engine.shutdown().await;
            engine?;
            coordinator?;
        }
        self.output
            .send(AppEvent::EngineStatusChanged(crate::EngineStatus::Stopped));
        Ok(())
    }

    async fn restore_benchmark_analysis(&mut self) {
        self.start_engine().await;
        if std::mem::take(&mut self.benchmark.resume_review)
            && self
                .review
                .progress
                .as_ref()
                .is_some_and(|p| p.status == WholeGameStatus::Paused)
        {
            // The normal resume path checks document keys and skips completed nodes.
            if let Err(error) = self.review_command(ReviewInput::Resume).await {
                self.analysis_failed(error.to_string());
            }
        }
    }

    pub(super) async fn stop_benchmark_process(&mut self) -> Result<(), AppError> {
        if let Some(process) = &mut self.benchmark.process {
            process.stop().await?;
        }
        self.benchmark.process = None;
        Ok(())
    }

    async fn finish_benchmark(&mut self, mut error: Option<String>, phase: BenchmarkPhase) {
        if let Err(cleanup) = self.stop_benchmark_process().await {
            error = Some(cleanup.to_string());
        }
        self.benchmark.status.phase = if error.is_some() {
            BenchmarkPhase::Failed
        } else {
            phase
        };
        self.benchmark.status.error = error;
        if self.benchmark.process.is_none() {
            self.restore_benchmark_analysis().await;
        }
        self.publish_benchmark().await;
    }

    pub(super) async fn benchmark_signal(&mut self, update: Result<Update, BenchmarkError>) {
        match update {
            Ok(Update::Progress {
                progress,
                measurements,
            }) => {
                self.benchmark.status.progress = Some(progress);
                self.benchmark.status.measurements = measurements;
                self.publish_benchmark().await;
            }
            Ok(Update::Finished {
                metrics,
                measurements,
            }) => {
                self.benchmark.status.measurements = measurements;
                let result = self.save_benchmark(metrics).await;
                self.finish_benchmark(
                    result.err().map(|e| e.to_string()),
                    BenchmarkPhase::Completed,
                )
                .await;
            }
            Err(error) => {
                self.finish_benchmark(Some(error.to_string()), BenchmarkPhase::Failed)
                    .await
            }
        }
    }

    async fn save_benchmark(
        &mut self,
        metrics: katago_protocol::benchmark::BenchmarkMetrics,
    ) -> Result<(), AppError> {
        let profile = self
            .settings
            .engine
            .as_ref()
            .ok_or(BenchmarkError::NoEngine)?;
        let fingerprint = profile.fingerprint().await?;
        if self.benchmark.fingerprint.as_ref() != Some(&fingerprint) {
            return Err(BenchmarkError::ChangedFiles.into());
        }
        let measured_at_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| BenchmarkError::Failed(e.to_string()))?
            .as_millis();
        let result = BenchmarkResult {
            engine_fingerprint: fingerprint.clone(),
            recommended_threads: metrics.threads,
            nn_max_batch_size: metrics.nn_max_batch_size,
            num_nn_server_threads_per_model: metrics.num_nn_server_threads_per_model,
            visits_per_second: metrics.visits_per_second,
            nn_evals_per_second: metrics.nn_evals_per_second,
            measured_at_unix_ms: u64::try_from(measured_at_unix_ms)
                .map_err(|e| BenchmarkError::Failed(e.to_string()))?,
            canonical_model_name: profile.canonical_model_name.clone(),
        };
        if !result.valid() {
            return Err(BenchmarkError::Failed("invalid recommendation".into()).into());
        }
        let mut settings = self.settings.clone();
        settings.benchmarks.insert(fingerprint, result);
        settings.save(&self.directory).await?;
        self.settings = settings;
        Ok(())
    }

    async fn reset_benchmark(&mut self) -> Result<(), AppError> {
        self.ensure_no_benchmark()?;
        let profile = self
            .settings
            .engine
            .as_ref()
            .ok_or(BenchmarkError::NoEngine)?;
        let fingerprint = profile.fingerprint().await?;
        let mut settings = self.settings.clone();
        let changed = settings.benchmarks.remove(&fingerprint).is_some();
        if changed {
            settings.save(&self.directory).await?;
            self.settings = settings;
            // A pending Ready-before-adopt candidate may already have the old
            // managed overrides. It must never take over after reset succeeds.
            self.auto_setup.cancel();
            self.cancel_trial().await;
            self.benchmark.resume_review = self.review_running();
            let suspended = self.suspend_benchmark_analysis().await;
            self.restore_benchmark_analysis().await;
            suspended?;
        }
        self.benchmark.fingerprint = Some(fingerprint);
        self.benchmark.profile = self.settings.engine.clone();
        self.benchmark.status = BenchmarkStatus::default();
        self.publish_benchmark().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
