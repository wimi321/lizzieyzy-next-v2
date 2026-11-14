use super::CommentaryError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CommentaryStyle {
    #[default]
    Balanced,
    Rigorous,
    Patient,
    Strict,
    Humorous,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CommentaryTerminology {
    Few,
    #[default]
    Medium,
    Many,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CommentaryDetail {
    Brief,
    #[default]
    Standard,
    Detailed,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CommentaryVariations {
    Few,
    #[default]
    Moderate,
    Many,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct CommentarySettings {
    #[ts(as = "Option<String>", optional)]
    pub base_url: String,
    #[ts(as = "Option<String>", optional)]
    pub model: String,
    #[ts(as = "Option<bool>", optional)]
    pub remember_key: bool,
    #[ts(as = "Option<String>", optional)]
    pub rank: String,
    #[ts(as = "Option<CommentaryStyle>", optional)]
    pub style: CommentaryStyle,
    #[ts(as = "Option<CommentaryTerminology>", optional)]
    pub terminology: CommentaryTerminology,
    #[ts(as = "Option<CommentaryDetail>", optional)]
    pub detail: CommentaryDetail,
    #[ts(as = "Option<CommentaryVariations>", optional)]
    pub variations: CommentaryVariations,
    #[ts(as = "Option<bool>", optional)]
    pub write_to_comment: bool,
}
impl Default for CommentarySettings {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            model: String::new(),
            remember_key: false,
            rank: "5k".into(),
            style: Default::default(),
            terminology: Default::default(),
            detail: Default::default(),
            variations: Default::default(),
            write_to_comment: true,
        }
    }
}
impl CommentarySettings {
    pub fn normalized(mut self) -> Result<Self, CommentaryError> {
        if !self.base_url.is_empty() {
            self.base_url = crate::commentary_client::normalize_base_url(&self.base_url)?;
        }
        if self.model.chars().count() > 256
            || self.model.chars().any(char::is_control)
            || self.model.trim() != self.model
        {
            return Err(CommentaryError::InvalidSettings("model".into()));
        }
        let valid_rank = (1..=18).any(|n| self.rank == format!("{n}k"))
            || (1..=9).any(|n| self.rank == format!("{n}d"));
        if !valid_rank {
            return Err(CommentaryError::InvalidSettings("rank".into()));
        }
        Ok(self)
    }
}

pub(crate) fn language(value: crate::UiLanguage) -> ::commentary::Language {
    use ::commentary::Language;
    match value.resolved() {
        crate::UiLanguage::ZhCn => Language::ZhCn,
        crate::UiLanguage::ZhTw => Language::ZhTw,
        crate::UiLanguage::Ja => Language::Ja,
        crate::UiLanguage::Ko => Language::Ko,
        crate::UiLanguage::Th => Language::Th,
        _ => Language::En,
    }
}
impl CommentarySettings {
    pub(crate) fn preferences(&self) -> ::commentary::Preferences {
        use ::commentary::{Detail, Style, Terminology, Variations};
        ::commentary::Preferences {
            rank: self.rank.clone(),
            style: match self.style {
                CommentaryStyle::Balanced => Style::Balanced,
                CommentaryStyle::Rigorous => Style::Rigorous,
                CommentaryStyle::Patient => Style::Patient,
                CommentaryStyle::Strict => Style::Strict,
                CommentaryStyle::Humorous => Style::Humorous,
            },
            terminology: match self.terminology {
                CommentaryTerminology::Few => Terminology::Few,
                CommentaryTerminology::Medium => Terminology::Medium,
                CommentaryTerminology::Many => Terminology::Many,
            },
            detail: match self.detail {
                CommentaryDetail::Brief => Detail::Brief,
                CommentaryDetail::Standard => Detail::Standard,
                CommentaryDetail::Detailed => Detail::Detailed,
            },
            variations: match self.variations {
                CommentaryVariations::Few => Variations::Few,
                CommentaryVariations::Moderate => Variations::Moderate,
                CommentaryVariations::Many => Variations::Many,
            },
        }
    }
}
