use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Nicknames consisting of digits still resolve as nicknames, never as UIDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "value")]
pub enum FoxQuery {
    Nickname(String),
    Uid(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FoxAccount {
    pub uid: String,
    pub nickname: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct FoxSearchResult {
    pub account: FoxAccount,
    pub page: FoxPage,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct FoxPage {
    pub games: Vec<FoxGame>,
    /// None means that the server's final page was reached.
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct FoxGame {
    pub id: String,
    pub start_time: String,
    pub black: FoxPlayer,
    pub white: FoxPlayer,
    pub result: FoxResult,
    pub move_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FoxPlayer {
    pub uid: String,
    pub nickname: String,
    pub rank: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct FoxResult {
    pub winner: Option<FoxColor>,
    pub kind: FoxResultKind,
    pub score: Option<f64>,
    pub unit: Option<FoxScoreUnit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum FoxColor {
    Black,
    White,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum FoxResultKind {
    Resignation,
    Timeout,
    Score,
    OtherWin,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum FoxScoreUnit {
    Points,
    Stones,
}
