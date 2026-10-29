use crate::Player;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResponse {
    pub id: String,
    pub turn_number: u32,
    pub is_during_search: bool,
    pub move_infos: Vec<MoveInfo>,
    pub root_info: RootInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownership: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<Vec<f64>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoResultsResponse {
    pub id: String,
    pub turn_number: u32,
    pub is_during_search: bool,
    pub no_results: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveInfo {
    #[serde(rename = "move")]
    pub coordinate: String,
    pub visits: u64,
    pub winrate: f64,
    pub score_lead: f64,
    pub prior: f64,
    pub order: u32,
    pub pv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_visits: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_mean: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_selfplay: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_stdev: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utility: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lcb: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utility_lcb: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play_selection_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_symmetry_of: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootInfo {
    pub visits: u64,
    pub winrate: f64,
    pub score_lead: f64,
    pub utility: f64,
    pub current_player: Player,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_selfplay: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_stdev: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub this_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sym_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_winrate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_lead: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_score_selfplay: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_score_selfplay_stdev: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_no_result_prob: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_st_score_error: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_st_wr_error: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_var_time_left: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorLine {
    pub error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarningLine {
    pub warning: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum ActionResponse {
    #[serde(rename = "query_version")]
    QueryVersion {
        id: String,
        version: String,
        git_hash: String,
    },
    #[serde(rename = "terminate")]
    Terminate {
        id: String,
        #[serde(rename = "terminateId")]
        terminate_id: String,
    },
    #[serde(rename = "terminate_all")]
    TerminateAll { id: String },
}

/// `NoResults` is a terminal cancellation response, never an analysis with
/// fabricated empty data. Action acknowledgements are separate from results.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum EngineLine {
    Analysis(Box<AnalysisResponse>),
    NoResults(NoResultsResponse),
    Error(ErrorLine),
    Warning(WarningLine),
    Action(ActionResponse),
}
