use crate::AnalysisError;
use serde_json::{Map, Value, json};

/// Search overrides shared by every analysis request. A missing value leaves
/// the engine's configured default intact.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SearchParams {
    pub playout_doubling_advantage: Option<f64>,
    pub wide_root_noise: Option<f64>,
    pub root_policy_temperature: Option<f64>,
}

impl SearchParams {
    /// Bounds follow KataGo's Analysis_Engine.md and cpp/program/setup.cpp.
    pub fn validate(&self) -> Result<(), AnalysisError> {
        for (value, low, high, message) in [
            (
                self.playout_doubling_advantage,
                -3.0,
                3.0,
                "PDA must be finite and within -3..=3",
            ),
            (
                self.wide_root_noise,
                0.0,
                5.0,
                "wide root noise must be finite and within 0..=5",
            ),
            (
                self.root_policy_temperature,
                0.01,
                100.0,
                "root policy temperature must be finite and within 0.01..=100",
            ),
        ] {
            if value.is_some_and(|value| !value.is_finite() || !(low..=high).contains(&value)) {
                return Err(AnalysisError::InvalidSettings(message));
            }
        }
        Ok(())
    }

    pub fn overrides(&self) -> Map<String, Value> {
        [
            ("playoutDoublingAdvantage", self.playout_doubling_advantage),
            ("wideRootNoise", self.wide_root_noise),
            ("rootPolicyTemperature", self.root_policy_temperature),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), json!(value))))
        .collect()
    }
}

#[cfg(test)]
mod tests;
