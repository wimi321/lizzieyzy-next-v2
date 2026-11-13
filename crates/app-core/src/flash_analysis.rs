//! Persisted fine-analysis defaults; rules remain part of EngineSettings.
use crate::{AppCore, AppError, WholeGameError, WholeGameOptions, actor};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct FlashAnalysisSettings {
    #[ts(as = "Option<u32>", optional)]
    pub visits: u64,
    #[ts(as = "Option<bool>", optional)]
    pub always_override: bool,
}

impl Default for FlashAnalysisSettings {
    fn default() -> Self {
        Self {
            visits: 1,
            always_override: false,
        }
    }
}

impl FlashAnalysisSettings {
    pub fn validate(self) -> Result<(), WholeGameError> {
        WholeGameOptions {
            visits: self.visits,
            ..Default::default()
        }
        .validate()
    }
}

impl AppCore {
    pub async fn flash_analysis(&self) -> Result<FlashAnalysisSettings, AppError> {
        Ok(self.settings().await?.flash_analysis)
    }

    pub async fn set_flash_analysis(
        &self,
        settings: FlashAnalysisSettings,
    ) -> Result<FlashAnalysisSettings, AppError> {
        self.send(actor::Input::SetFlashAnalysis(settings)).await?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests;
