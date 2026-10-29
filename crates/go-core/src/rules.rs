use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KoRule {
    Simple,
    PositionalSuperko,
    SituationalSuperko,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScoringRule {
    Area,
    Territory,
}

/// Rule mechanics needed to play moves. Scoring itself is left to KataGo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rules {
    pub ko: KoRule,
    /// KataGo's `multiStoneSuicideLegal`: permits removing a friendly group of
    /// at least two stones. Single-stone suicide is always illegal.
    pub allow_multi_stone_suicide: bool,
    pub scoring: ScoringRule,
}

impl Rules {
    pub fn from_name(name: &str) -> Result<Self, UnknownRules> {
        Ok(RulePreset::from_name(name)?.rules())
    }
}

/// Preset identity is kept separately: Japanese and Korean share move mechanics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RulePreset {
    Chinese,
    Japanese,
    Korean,
    Aga,
    TrompTaylor,
    NewZealand,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unsupported rules preset {0:?}")]
pub struct UnknownRules(pub String);

impl RulePreset {
    /// Accept exactly the six canonical names supported by this crate.
    pub fn from_name(name: &str) -> Result<Self, UnknownRules> {
        match name {
            "chinese" => Ok(Self::Chinese),
            "japanese" => Ok(Self::Japanese),
            "korean" => Ok(Self::Korean),
            "aga" => Ok(Self::Aga),
            "tromp-taylor" => Ok(Self::TrompTaylor),
            "new-zealand" => Ok(Self::NewZealand),
            _ => Err(UnknownRules(name.to_owned())),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Chinese => "chinese",
            Self::Japanese => "japanese",
            Self::Korean => "korean",
            Self::Aga => "aga",
            Self::TrompTaylor => "tromp-taylor",
            Self::NewZealand => "new-zealand",
        }
    }

    /// Pass this named preset through to KataGo to preserve its scoring options.
    pub const fn as_katago_str(self) -> &'static str {
        self.name()
    }

    /// Maps the play-related subset of KataGo's named presets.
    /// See KataGo's `cpp/game/rules.cpp`, `parseRulesHelper`.
    pub const fn rules(self) -> Rules {
        match self {
            Self::Chinese => Rules {
                ko: KoRule::Simple,
                allow_multi_stone_suicide: false,
                scoring: ScoringRule::Area,
            },
            Self::Japanese | Self::Korean => Rules {
                ko: KoRule::Simple,
                allow_multi_stone_suicide: false,
                scoring: ScoringRule::Territory,
            },
            Self::Aga => Rules {
                ko: KoRule::SituationalSuperko,
                allow_multi_stone_suicide: false,
                scoring: ScoringRule::Area,
            },
            Self::TrompTaylor => Rules {
                ko: KoRule::PositionalSuperko,
                allow_multi_stone_suicide: true,
                scoring: ScoringRule::Area,
            },
            Self::NewZealand => Rules {
                ko: KoRule::SituationalSuperko,
                allow_multi_stone_suicide: true,
                scoring: ScoringRule::Area,
            },
        }
    }
}

#[cfg(test)]
mod tests;
