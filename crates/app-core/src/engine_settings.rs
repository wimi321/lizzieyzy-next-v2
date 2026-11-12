//! Persistent analysis choices; these never rewrite an imported game.

use crate::{AppCore, AppError, SettingsError, actor::Command};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum EngineRules {
    #[default]
    FollowSgf,
    Chinese,
    Japanese,
    Korean,
    Aga,
    TrompTaylor,
    NewZealand,
    ChineseOgs,
    ChineseKgs,
    StoneScoring,
    Bga,
    AgaButton,
    Goe,
    AncientTerritory,
    Custom(EngineCustomRules),
}

/// The seven fields exposed by Java's KataRulesEditor, with KataGo wire names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineCustomRules {
    #[ts(as = "Option<String>", optional)]
    pub ko: String,
    #[ts(as = "Option<String>", optional)]
    pub scoring: String,
    #[ts(as = "Option<String>", optional)]
    pub tax: String,
    #[ts(as = "Option<bool>", optional)]
    pub suicide: bool,
    #[ts(as = "Option<bool>", optional)]
    pub has_button: bool,
    #[ts(as = "Option<String>", optional)]
    pub white_handicap_bonus: String,
    #[ts(as = "Option<bool>", optional)]
    pub friendly_pass_ok: bool,
}

impl EngineRules {
    pub fn from_java(raw: &str) -> Result<Self, SettingsError> {
        let raw = raw.trim().trim_start_matches('=').trim();
        if raw.starts_with('{') {
            let rules = Self::Custom(serde_json::from_str(raw)?);
            rules.validate()?;
            return Ok(rules);
        }
        Ok(match raw.to_ascii_lowercase().as_str() {
            "chinese" | "cn" => Self::Chinese,
            "japanese" | "jp" => Self::Japanese,
            "korean" | "kr" => Self::Korean,
            "aga" | "french" => Self::Aga,
            "tromp-taylor" | "tromptaylor" => Self::TrompTaylor,
            "new-zealand" | "newzealand" | "nz" => Self::NewZealand,
            "chinese-ogs" | "chineseogs" => Self::ChineseOgs,
            "chinese-kgs" | "chinesekgs" => Self::ChineseKgs,
            "stone-scoring" | "stonescoring" | "ancient-area" | "ancientarea" => Self::StoneScoring,
            "bga" => Self::Bga,
            "aga-button" | "agabutton" => Self::AgaButton,
            "goe" | "ing" => Self::Goe,
            "ancient-territory" | "ancientterritory" => Self::AncientTerritory,
            _ => return Err(SettingsError::InvalidProfile("unknown engine rules".into())),
        })
    }

    pub(crate) fn validate(&self) -> Result<(), SettingsError> {
        if let Self::Custom(rules) = self
            && (!matches!(rules.ko.as_str(), "SIMPLE" | "POSITIONAL" | "SITUATIONAL")
                || !matches!(rules.scoring.as_str(), "AREA" | "TERRITORY")
                || !matches!(rules.tax.as_str(), "NONE" | "SEKI" | "ALL")
                || !matches!(rules.white_handicap_bonus.as_str(), "0" | "N" | "N-1")
                || (rules.has_button && rules.scoring != "AREA"))
        {
            return Err(SettingsError::InvalidProfile(
                "invalid custom engine rules".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn query_rules(&self) -> Option<katago_protocol::Rules> {
        use katago_protocol::Rules;
        let name = match self {
            Self::FollowSgf => return None,
            Self::Chinese => "chinese",
            Self::Japanese => "japanese",
            Self::Korean => "korean",
            Self::Aga => "aga",
            Self::TrompTaylor => "tromp-taylor",
            Self::NewZealand => "new-zealand",
            Self::ChineseOgs => "chinese-ogs",
            Self::ChineseKgs => "chinese-kgs",
            Self::StoneScoring => "stone-scoring",
            Self::Bga => "bga",
            Self::AgaButton => "aga-button",
            Self::Goe => "goe",
            Self::AncientTerritory => "ancient-territory",
            Self::Custom(rules) => {
                let serde_json::Value::Object(fields) = serde_json::json!(rules) else {
                    return None;
                };
                return Some(Rules::Custom(fields));
            }
        };
        Some(Rules::Named(name.into()))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct EngineSettings {
    #[ts(as = "Option<EngineRules>", optional)]
    pub rules: EngineRules,
    #[ts(as = "Option<bool>", optional)]
    pub read_komi: bool,
    #[ts(as = "Option<f64>", optional)]
    pub default_komi: f64,
    #[ts(as = "Option<bool>", optional)]
    pub auto_check_alive: bool,
    #[ts(as = "Option<u32>", optional)]
    pub report_interval_ms: u32,
}

impl Default for EngineSettings {
    fn default() -> Self {
        Self {
            rules: EngineRules::FollowSgf,
            read_komi: true,
            default_komi: 7.5,
            auto_check_alive: true,
            report_interval_ms: 100,
        }
    }
}

impl EngineSettings {
    pub fn validate(&self) -> Result<(), SettingsError> {
        self.rules.validate()?;
        if !self.default_komi.is_finite() || self.default_komi.abs() > 400.0 {
            return Err(SettingsError::InvalidProfile(
                "default komi must be finite and within -400..400".into(),
            ));
        }
        if self.report_interval_ms == 0 || self.report_interval_ms > 86_400_000 {
            return Err(SettingsError::InvalidProfile(
                "report interval must be in (0, 24h]".into(),
            ));
        }
        Ok(())
    }

    /// A versioned suffix isolates actual query overrides from document keys.
    /// Report cadence and watchdog policy do not change evaluation identity.
    pub(crate) fn evaluation_identity(&self) -> Result<Vec<u8>, SettingsError> {
        Ok(serde_json::to_vec(&(
            "engine-query-settings:v1",
            &self.rules,
            (!self.read_komi).then_some(self.default_komi),
        ))?)
    }
}

impl AppCore {
    pub async fn set_engine_settings(
        &self,
        settings: EngineSettings,
    ) -> Result<EngineSettings, AppError> {
        let (reply, received) = oneshot::channel();
        self.commands
            .send(Command::SetEngineSettings(settings, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        received.await.map_err(|_| AppError::Closed)?
    }
}

#[cfg(test)]
mod tests;
