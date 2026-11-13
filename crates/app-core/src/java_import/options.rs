use super::display::{boolean, integer};
use crate::{AppError, EngineRules, EngineSettings};
use serde_json::Value;

pub(super) fn merge(
    ui: &Value,
    leelaz: &Value,
    settings: &mut EngineSettings,
    issues: &mut Vec<String>,
) -> Result<(), AppError> {
    if let Some(value) = ui.get("read-komi") {
        settings.read_komi = boolean(value, "read-komi")?;
    }
    if let Some(value) = ui.get("new-game-komi") {
        settings.default_komi = value
            .as_f64()
            .ok_or_else(|| super::invalid("new-game-komi must be numeric"))?;
    }
    if let Some(value) = ui.get("auto-check-engine-alive") {
        settings.auto_check_alive = boolean(value, "auto-check-engine-alive")?;
    }
    if let Some(value) = leelaz.get("analyze-update-interval-centisec") {
        settings.report_interval_ms = integer(value, "analyze-update-interval-centisec")?
            .checked_mul(10)
            .and_then(|value| value.try_into().ok())
            .ok_or_else(|| super::invalid("invalid local analysis interval"))?;
    }
    if ui.get("auto-load-kata-rules").is_some() || ui.get("kata-rules").is_some() {
        let enabled = ui
            .get("auto-load-kata-rules")
            .map(|value| boolean(value, "auto-load-kata-rules"))
            .transpose()?
            .unwrap_or(false);
        if enabled {
            let text = ui["kata-rules"]
                .as_str()
                .ok_or_else(|| super::invalid("kata-rules must be text"))?;
            settings.rules = EngineRules::from_java(text)?;
        } else {
            settings.rules = EngineRules::FollowSgf;
            if ui["kata-rules"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
            {
                issues.push("saved kata-rules was inactive in Java; FollowSgf was imported".into());
            }
        }
    }
    settings.validate()?;
    Ok(())
}
