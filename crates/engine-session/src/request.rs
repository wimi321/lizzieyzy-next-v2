use crate::{QueryId, SessionError};
use katago_protocol::{AnalysisQuery, Player, Rules};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// A query's moves use KataGo coordinates (including `pass`) without applying
/// Go rules here; legality is determined by the engine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRequest {
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

pub(crate) const MAX_TURNS: usize = 256;

impl AnalysisRequest {
    pub(crate) fn validate(&self) -> Result<(), SessionError> {
        const RESERVED: &[&str] = &[
            "id",
            "action",
            "terminateId",
            "moves",
            "rules",
            "boardXSize",
            "boardYSize",
            "initialStones",
            "initialPlayer",
            "komi",
            "analyzeTurns",
            "maxVisits",
            "includeOwnership",
            "includePolicy",
            "reportDuringSearchEvery",
            "firstReportDuringSearchAfter",
            "priority",
            "priorities",
            "overrideSettings",
        ];
        if self
            .extensions
            .keys()
            .any(|key| RESERVED.contains(&key.as_str()))
        {
            return Err(SessionError::InvalidRequest(
                "reserved extension field".into(),
            ));
        }
        if self.moves.len() > u32::MAX as usize
            || self.analyze_turns.as_ref().is_some_and(|turns| {
                turns.is_empty()
                    || turns.len() > MAX_TURNS
                    || turns.iter().any(|turn| *turn as usize > self.moves.len())
            })
        {
            return Err(SessionError::InvalidRequest(
                "analyzeTurns must contain 1..=256 valid turns".into(),
            ));
        }
        if self.priorities.as_ref().is_some_and(|priorities| {
            self.priority.is_some()
                || self
                    .analyze_turns
                    .as_ref()
                    .is_none_or(|turns| turns.len() != priorities.len())
        }) {
            return Err(SessionError::InvalidRequest(
                "priorities must match analyzeTurns and exclude priority".into(),
            ));
        }
        if [
            self.komi,
            self.report_during_search_every,
            self.first_report_during_search_after,
        ]
        .into_iter()
        .flatten()
        .any(|v| !v.is_finite())
        {
            return Err(SessionError::InvalidRequest(
                "nonfinite query number".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn turns(&self) -> BTreeSet<u32> {
        self.analyze_turns
            .clone()
            .unwrap_or_else(|| vec![self.moves.len() as u32])
            .into_iter()
            .collect()
    }

    pub(crate) fn query(&self, id: QueryId, remaining: &BTreeSet<u32>) -> AnalysisQuery {
        let mut query = AnalysisQuery {
            id,
            moves: self.moves.clone(),
            rules: self.rules.clone(),
            board_x_size: self.board_x_size,
            board_y_size: self.board_y_size,
            initial_stones: self.initial_stones.clone(),
            initial_player: self.initial_player,
            komi: self.komi,
            analyze_turns: self.analyze_turns.clone(),
            max_visits: self.max_visits,
            include_ownership: self.include_ownership,
            include_policy: self.include_policy,
            report_during_search_every: self.report_during_search_every,
            first_report_during_search_after: self.first_report_during_search_after,
            priority: self.priority,
            priorities: self.priorities.clone(),
            override_settings: self.override_settings.clone(),
            extensions: self.extensions.clone(),
        };
        if let Some(turns) = &self.analyze_turns {
            query.analyze_turns = Some(
                turns
                    .iter()
                    .copied()
                    .filter(|t| remaining.contains(t))
                    .collect(),
            );
            if let Some(priorities) = &self.priorities {
                query.priorities = Some(
                    turns
                        .iter()
                        .zip(priorities)
                        .filter_map(|(t, p)| remaining.contains(t).then_some(*p))
                        .collect(),
                );
            }
        }
        query
    }
}
