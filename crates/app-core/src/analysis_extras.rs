//! Transient display payload requests; never persisted in Settings.

use crate::{AppCore, AppError, actor};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct AnalysisExtras {
    #[ts(as = "Option<bool>", optional)]
    pub policy: bool,
    #[ts(as = "Option<bool>", optional)]
    pub moves_ownership: bool,
}

impl From<AnalysisExtras> for analysis::AnalysisExtras {
    fn from(value: AnalysisExtras) -> Self {
        Self {
            policy: value.policy,
            moves_ownership: value.moves_ownership,
        }
    }
}

impl AppCore {
    pub async fn set_analysis_extras(&self, extras: AnalysisExtras) -> Result<(), AppError> {
        self.send(actor::Input::SetAnalysisExtras(extras))
            .await
            .map(|_| ())
    }

    pub async fn set_pure_network(&self, enabled: bool) -> Result<(), AppError> {
        self.send(actor::Input::SetPureNetwork(enabled))
            .await
            .map(|_| ())
    }
}
