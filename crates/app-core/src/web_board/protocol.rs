use crate::{AnalysisView, BoardView, GraphView, ProblemMovesView};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct WebBoardOptions {
    #[ts(as = "Option<bool>", optional)]
    pub allow_trial: bool,
}
impl Default for WebBoardOptions {
    fn default() -> Self {
        Self { allow_trial: true }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WebBoardStatus {
    #[ts(as = "Option<bool>", optional)]
    pub running: bool,
    #[ts(optional = nullable)]
    pub port: Option<u16>,
    #[ts(optional = nullable)]
    pub url: Option<String>,
    #[ts(as = "Option<Vec<String>>", optional)]
    pub addresses: Vec<String>,
    #[ts(as = "Option<usize>", optional)]
    pub viewers: usize,
    #[ts(optional = nullable)]
    pub trial: Option<WebTrialOwner>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WebTrialOwner {
    #[ts(as = "Option<String>", optional)]
    pub owner: String,
    /// Unix milliseconds; never used for expiry decisions.
    #[ts(as = "Option<f64>", optional)]
    pub since: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct WebHello {
    #[ts(as = "Option<String>", optional)]
    pub client_id: String,
    /// Bearer credential returned only to this connection, never broadcast.
    #[ts(as = "Option<String>", optional)]
    pub reconnect: String,
    #[ts(as = "Option<f64>", optional)]
    pub komi: f64,
    #[ts(as = "Option<String>", optional)]
    pub rules: String,
    #[ts(as = "Option<bool>", optional)]
    pub allow_trial: bool,
}

/// Identical board/analysis DTOs to the host. No SGF, comments or file paths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct WebFullState {
    #[ts(as = "Option<f64>", optional)]
    pub revision: u64,
    #[ts(as = "Option<u32>", optional)]
    pub node_id: u32,
    #[ts(optional = nullable)]
    pub board: Option<BoardView>,
    #[ts(optional = nullable)]
    pub analysis: Option<AnalysisView>,
}

/// Analysis carries the same focus identity as FullState and no stone array.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct WebAnalysisUpdate {
    #[ts(as = "Option<f64>", optional)]
    pub revision: u64,
    #[ts(as = "Option<u32>", optional)]
    pub node_id: u32,
    #[ts(optional = nullable)]
    pub board: Option<WebPositionView>,
    #[ts(optional = nullable)]
    pub analysis: Option<AnalysisView>,
}

/// Board metadata without the stone arrays, copied from the host snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WebPositionView {
    #[ts(as = "Option<u8>", optional)]
    pub width: u8,
    #[ts(as = "Option<u8>", optional)]
    pub height: u8,
    #[ts(optional = nullable)]
    pub last_move: Option<crate::MoveView>,
    #[ts(as = "Option<crate::ColorView>", optional)]
    pub to_play: crate::ColorView,
    #[ts(as = "Option<usize>", optional)]
    pub move_number: usize,
    #[ts(as = "Option<usize>", optional)]
    pub captured_black: usize,
    #[ts(as = "Option<usize>", optional)]
    pub captured_white: usize,
}

impl From<&BoardView> for WebPositionView {
    fn from(board: &BoardView) -> Self {
        Self {
            width: board.width,
            height: board.height,
            last_move: board.last_move,
            to_play: board.to_play,
            move_number: board.move_number,
            captured_black: board.captured_black,
            captured_white: board.captured_white,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WebTrialState {
    #[ts(as = "Option<bool>", optional)]
    pub active: bool,
    #[ts(optional = nullable)]
    pub owner: Option<String>,
    #[ts(as = "Option<usize>", optional)]
    pub anchor_move: usize,
    #[ts(as = "Option<usize>", optional)]
    pub move_number: usize,
    #[ts(as = "Option<bool>", optional)]
    pub can_back: bool,
    #[ts(as = "Option<bool>", optional)]
    pub can_forward: bool,
    #[ts(as = "Option<Vec<WebTrialBranch>>", optional)]
    pub branches: Vec<WebTrialBranch>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WebTrialBranch {
    /// Zero-based x from the left, y from the top. Both null for pass/empty nodes.
    #[ts(optional = nullable)]
    pub x: Option<u8>,
    #[ts(optional = nullable)]
    pub y: Option<u8>,
    #[ts(as = "Option<String>", optional)]
    pub label: String,
    #[ts(as = "Option<usize>", optional)]
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum WebTrialDeniedReason {
    InUse,
    Disabled,
    HostBusy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum WebServerMessage {
    Hello(WebHello),
    FullState(WebFullState),
    AnalysisUpdate(WebAnalysisUpdate),
    WinrateHistory(GraphView),
    ProblemMoves(ProblemMovesView),
    TrialState(WebTrialState),
    TrialDenied { reason: WebTrialDeniedReason },
    TrialRejected { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum WebClientMessage {
    EnterTrial,
    ExitTrial,
    /// Left/top origin, zero-based. Rules and ko are checked by document/go-core.
    TrialMove {
        x: u8,
        y: u8,
    },
    TrialBack,
    TrialForward,
    TrialToAnchor,
    TrialChild {
        index: usize,
    },
}

impl WebClientMessage {
    /// Decode one WebSocket text message after the transport's size/UTF-8 checks.
    pub(crate) fn parse(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}
