//! Persisted release preferences. Selecting a language never changes engine identity.
use crate::{AppCore, AppError, EngineDisplayName, EngineProfile, actor};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum UiLanguage {
    #[default]
    System,
    ZhCn,
    En,
}

impl UiLanguage {
    pub fn resolved(self) -> Self {
        self.resolve_locale(sys_locale::get_locale().as_deref())
    }

    fn resolve_locale(self, locale: Option<&str>) -> Self {
        match self {
            Self::System
                if locale.is_some_and(|value| value.to_ascii_lowercase().starts_with("zh")) =>
            {
                Self::ZhCn
            }
            Self::System => Self::En,
            explicit => explicit,
        }
    }
}

impl AppCore {
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
        ("b10-lightweight", _) => "Transformer 10B Lightweight",
        ("b10-balanced", _) => "Transformer 10B Balanced",
        ("b11-flagship", _) => "Transformer 11B Flagship",
        _ => return name,
    };
    EngineDisplayName::Text(text.into())
}

#[cfg(test)]
mod tests;
