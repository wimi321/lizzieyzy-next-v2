//! Persisted release preferences. Selecting a language never changes engine identity.
use crate::{AppCore, AppError, EngineDisplayName, EngineProfile, actor};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum UiLanguage {
    #[default]
    System,
    ZhCn,
    ZhTw,
    Ja,
    Ko,
    Th,
    En,
}

impl UiLanguage {
    pub fn resolved(self) -> Self {
        self.resolve_locale(sys_locale::get_locale().as_deref())
    }

    pub fn resolve_locale(self, locale: Option<&str>) -> Self {
        if self != Self::System {
            return self;
        }
        let locale = locale.unwrap_or_default().to_ascii_lowercase();
        let locale = locale.split(['.', '@']).next().unwrap_or_default();
        let mut parts = locale.split(['-', '_']);
        match parts.next() {
            Some("zh") if parts.any(|part| matches!(part, "tw" | "hk" | "hant")) => Self::ZhTw,
            Some("zh") => Self::ZhCn,
            Some("ja") => Self::Ja,
            Some("ko") => Self::Ko,
            Some("th") => Self::Th,
            _ => Self::En,
        }
    }
}

impl AppCore {
    pub async fn always_on_top(&self) -> Result<bool, AppError> {
        Ok(self.settings().await?.always_on_top)
    }

    pub async fn set_always_on_top(&self, enabled: bool) -> Result<bool, AppError> {
        self.send(actor::Input::SetAlwaysOnTop(enabled)).await?;
        Ok(enabled)
    }

    pub async fn ui_scale(&self) -> Result<f64, AppError> {
        Ok(self.settings().await?.ui_scale)
    }

    pub async fn set_ui_scale(&self, scale: f64) -> Result<f64, AppError> {
        self.send(actor::Input::SetUiScale(scale)).await?;
        Ok(scale)
    }

    pub async fn play_sound(&self) -> Result<bool, AppError> {
        Ok(self.settings().await?.play_sound)
    }

    pub async fn set_play_sound(&self, enabled: bool) -> Result<bool, AppError> {
        self.send(actor::Input::SetPlaySound(enabled)).await?;
        Ok(enabled)
    }

    pub async fn set_language(&self, language: UiLanguage) -> Result<UiLanguage, AppError> {
        self.send(actor::Input::SetLanguage(language)).await?;
        Ok(language)
    }

    pub async fn set_auto_check_updates(&self, enabled: bool) -> Result<bool, AppError> {
        self.send(actor::Input::SetAutoCheckUpdates(enabled))
            .await?;
        Ok(enabled)
    }

    pub async fn localized_engine_display_name(
        &self,
        profile: &EngineProfile,
    ) -> Result<EngineDisplayName, AppError> {
        Ok(localize_engine_name(
            crate::engine_display_name(profile),
            self.settings().await?.language,
        ))
    }

    /// Claim before networking so failed checks and concurrent launches cannot
    /// repeatedly contact the endpoint. A clock moving backwards is conservative.
    pub async fn claim_daily_update_check(&self, now_unix_ms: u64) -> Result<bool, AppError> {
        let (reply, response) = tokio::sync::oneshot::channel();
        self.commands
            .send(actor::Command::ClaimUpdateCheck(now_unix_ms, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

fn localize_engine_name(name: EngineDisplayName, language: UiLanguage) -> EngineDisplayName {
    // System retains the existing catalog contract: the UI already resolves its
    // system language. Explicit choices must also work for non-UI callers.
    let EngineDisplayName::Catalog(ref id) = name else {
        return name;
    };
    if language == UiLanguage::System {
        return name;
    }
    let text = match (id.as_str(), language) {
        ("b10-lightweight", UiLanguage::ZhCn) => "Transformer 10B 轻量版",
        ("b10-balanced", UiLanguage::ZhCn) => "Transformer 10B 均衡版",
        ("b11-flagship", UiLanguage::ZhCn) => "Transformer 11B 旗舰版",
        ("b10-lightweight", UiLanguage::ZhTw) => "Transformer 10B 輕量版",
        ("b10-balanced", UiLanguage::ZhTw) => "Transformer 10B 均衡版",
        ("b11-flagship", UiLanguage::ZhTw) => "Transformer 11B 旗艦版",
        ("b10-lightweight", UiLanguage::Ja) => "Transformer 10B 軽量版",
        ("b10-balanced", UiLanguage::Ja) => "Transformer 10B バランス版",
        ("b11-flagship", UiLanguage::Ja) => "Transformer 11B フラッグシップ",
        ("b10-lightweight", UiLanguage::Ko) => "Transformer 10B 경량형",
        ("b10-balanced", UiLanguage::Ko) => "Transformer 10B 균형형",
        ("b11-flagship", UiLanguage::Ko) => "Transformer 11B 플래그십",
        ("b10-lightweight", UiLanguage::Th) => "Transformer 10B รุ่นเบา",
        ("b10-balanced", UiLanguage::Th) => "Transformer 10B รุ่นสมดุล",
        ("b11-flagship", UiLanguage::Th) => "Transformer 11B รุ่นเรือธง",
        ("b10-lightweight", _) => "Transformer 10B Lightweight",
        ("b10-balanced", _) => "Transformer 10B Balanced",
        ("b11-flagship", _) => "Transformer 11B Flagship",
        _ => return name,
    };
    EngineDisplayName::Text(text.into())
}

#[cfg(test)]
mod tests;
