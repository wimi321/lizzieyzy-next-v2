use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Player {
    #[serde(rename = "B", alias = "b", alias = "BLACK", alias = "black")]
    Black,
    #[serde(rename = "W", alias = "w", alias = "WHITE", alias = "white")]
    White,
}

/// KataGo accepts both a named ruleset and a custom rules object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Rules {
    Named(String),
    Custom(Map<String, Value>),
}

/// A query's moves use KataGo coordinates (including `pass`) without applying
/// Go rules here; legality is determined by the engine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisQuery {
    pub id: String,
    pub moves: Vec<(Player, String)>,
    pub rules: Rules,
    pub board_x_size: u8,
    pub board_y_size: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_stones: Option<Vec<(Player, String)>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_player: Option<Player>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub komi: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analyze_turns: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_visits: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_ownership: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_policy: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_during_search_every: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_report_during_search_after: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priorities: Option<Vec<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_settings: Option<Map<String, Value>>,
    /// Outgoing extensions are retained for forward compatibility and exact
    /// replay of recorded requests. Incoming response extensions are ignored.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum Action {
    #[serde(rename = "query_version")]
    QueryVersion { id: String },
    #[serde(rename = "terminate")]
    Terminate {
        id: String,
        #[serde(rename = "terminateId")]
        terminate_id: String,
    },
    #[serde(rename = "terminate_all")]
    TerminateAll { id: String },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Request {
    Analysis(Box<AnalysisQuery>),
    Action(Action),
}

impl<'de> Deserialize<'de> for Request {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        if value.get("action").is_some() {
            serde_json::from_value(value)
                .map(Self::Action)
                .map_err(serde::de::Error::custom)
        } else {
            serde_json::from_value(value)
                .map(|query| Self::Analysis(Box::new(query)))
                .map_err(serde::de::Error::custom)
        }
    }
}
