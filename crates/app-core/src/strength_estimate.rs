//! Strength estimates from the currently displayed line's retained analysis.

use crate::{AppCore, AppError, actor};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct StrengthEstimateView {
    #[ts(type = "number")]
    pub revision: u64,
    pub report: strength::StrengthReport,
    pub phases: strength::StrengthPhasesReport,
    /// Includes moves lacking comparable analysis, so partial coverage is visible.
    pub total_moves: usize,
    pub analyzed_moves: usize,
}

impl AppCore {
    /// Re-read on DocumentChanged, AnalysisUpdated and WholeGameProgress.
    /// This command never starts analysis or changes the document/focus.
    pub async fn strength_estimate(
        &self,
        bounds: Option<[i32; 2]>,
    ) -> Result<StrengthEstimateView, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::StrengthEstimate(bounds, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

#[cfg(test)]
mod tests;
