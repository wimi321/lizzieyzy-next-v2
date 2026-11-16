use crate::{AnalysisView, AppCore, AppError, EngineStatus, actor};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct ComparisonView {
    pub engine_id: Option<String>,
    pub status: EngineStatus,
    pub analysis: Option<AnalysisView>,
}

impl AppCore {
    pub async fn comparison_state(&self) -> Result<ComparisonView, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::ComparisonState(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }
}
