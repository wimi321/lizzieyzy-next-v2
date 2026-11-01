//! User-initiated official KataGo benchmark; never part of automatic setup.
use crate::{AppCore, AppError, actor};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

pub(crate) mod process;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum BenchmarkPhase {
    #[default]
    Idle,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct BenchmarkProgress {
    pub threads: u32,
    pub completed_positions: u32,
    pub total_positions: u32,
    pub visits_per_second: Option<f64>,
    pub nn_evals_per_second: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct BenchmarkMeasurement {
    pub threads: u32,
    pub completed_positions: u32,
    pub total_positions: u32,
    pub visits_per_second: f64,
    pub nn_evals_per_second: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct BenchmarkResult {
    pub engine_fingerprint: String,
    pub recommended_threads: u32,
    /// Official additional recommendation; old saved results default to None.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub nn_max_batch_size: Option<u32>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub num_nn_server_threads_per_model: Option<u32>,
    pub visits_per_second: f64,
    pub nn_evals_per_second: f64,
    #[ts(type = "number")]
    pub measured_at_unix_ms: u64,
    /// None means the model has not been identified by the model catalog.
    pub canonical_model_name: Option<String>,
}

impl BenchmarkResult {
    pub(crate) fn valid(&self) -> bool {
        (1..=4096).contains(&self.recommended_threads)
            && self
                .nn_max_batch_size
                .is_none_or(|n| (1..=65536).contains(&n))
            && self
                .num_nn_server_threads_per_model
                .is_none_or(|n| (1..=256).contains(&n))
            && self.visits_per_second.is_finite()
            && self.visits_per_second > 0.0
            && self.nn_evals_per_second.is_finite()
            && self.nn_evals_per_second > 0.0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct BenchmarkStatus {
    pub phase: BenchmarkPhase,
    pub progress: Option<BenchmarkProgress>,
    pub measurements: Vec<BenchmarkMeasurement>,
    pub last_result: Option<BenchmarkResult>,
    pub error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BenchmarkError {
    #[error("benchmark is running; cancel it before changing analysis or engine settings")]
    Busy,
    #[error("no engine is configured")]
    NoEngine,
    #[error("benchmark failed: {0}")]
    Failed(String),
    #[error("engine files changed during benchmark; result was not saved")]
    ChangedFiles,
}

pub(crate) enum Request {
    Start,
    Cancel,
    Status,
    Reset,
}

impl AppCore {
    pub async fn start_benchmark(&self) -> Result<BenchmarkStatus, AppError> {
        self.benchmark_request(Request::Start).await
    }

    pub async fn cancel_benchmark(&self) -> Result<BenchmarkStatus, AppError> {
        self.benchmark_request(Request::Cancel).await
    }

    /// Latest progress and saved result for the current engine fingerprint.
    pub async fn benchmark_status(&self) -> Result<BenchmarkStatus, AppError> {
        self.benchmark_request(Request::Status).await
    }

    /// Remove only the current fingerprint's optimization, then restart analysis.
    pub async fn reset_benchmark(&self) -> Result<BenchmarkStatus, AppError> {
        self.benchmark_request(Request::Reset).await
    }

    async fn benchmark_request(&self, request: Request) -> Result<BenchmarkStatus, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::Benchmark(request, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}
