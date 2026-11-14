use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::TencentError;
use crate::FoxResult;

/// Numeric text is sent as both username and srcuid, preserving leading zeroes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TencentQuery(pub String);

impl TencentQuery {
    pub fn normalized(&self) -> Result<Self, TencentError> {
        let text = self.0.trim();
        if text.is_empty() || text.len() > 256 || text.chars().any(char::is_control) {
            Err(TencentError::InvalidQuery)
        } else {
            Ok(Self(text.to_owned()))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct TencentPage {
    #[ts(as = "Option<Vec<TencentGame>>", optional)]
    pub games: Vec<TencentGame>,
    #[ts(optional = nullable)]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct TencentGame {
    #[ts(as = "Option<String>", optional)]
    pub id: String,
    #[ts(as = "Option<f64>", optional = nullable)]
    pub start_time: Option<i64>,
    #[ts(as = "Option<TencentPlayer>", optional)]
    pub black: TencentPlayer,
    #[ts(as = "Option<TencentPlayer>", optional)]
    pub white: TencentPlayer,
    #[ts(as = "Option<FoxResult>", optional)]
    pub result: FoxResult,
    #[ts(as = "Option<u32>", optional)]
    pub move_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TencentPlayer {
    #[ts(as = "Option<String>", optional)]
    pub uid: String,
    #[ts(as = "Option<String>", optional)]
    pub nickname: String,
    #[ts(as = "Option<TencentRank>", optional)]
    pub rank: TencentRank,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum TencentRank {
    Kyu(u32),
    Dan(u32),
    Professional(u32),
}

impl TencentRank {
    pub fn from_raw(raw: i32) -> Self {
        if raw >= 100 {
            Self::Professional((i64::from(raw) - 99) as u32)
        } else if raw > 17 {
            Self::Dan((raw - 17) as u32)
        } else {
            Self::Kyu((18 - i64::from(raw)) as u32)
        }
    }
}
