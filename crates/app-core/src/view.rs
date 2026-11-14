//! Serializable rendering data. Go rules and analysis remain in the core crates.

use document::{DocError, GameDocument, GamePosition};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{AnalysisSummary, GameInfoView, GraphView};

pub(crate) fn score_available() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ColorView {
    Black,
    White,
}

impl From<go_core::Color> for ColorView {
    fn from(value: go_core::Color) -> Self {
        match value {
            go_core::Color::Black => Self::Black,
            go_core::Color::White => Self::White,
        }
    }
}

impl From<ColorView> for go_core::Color {
    fn from(value: ColorView) -> Self {
        match value {
            ColorView::Black => Self::Black,
            ColorView::White => Self::White,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PointView {
    pub x: u8,
    pub y: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum MoveView {
    Play { color: ColorView, point: PointView },
    Pass { color: ColorView },
}

impl From<go_core::Move> for MoveView {
    fn from(value: go_core::Move) -> Self {
        match value {
            go_core::Move::Play(color, point) => Self::Play {
                color: color.into(),
                point: PointView {
                    x: point.x,
                    y: point.y,
                },
            },
            go_core::Move::Pass(color) => Self::Pass {
                color: color.into(),
            },
        }
    }
}

impl From<MoveView> for go_core::Move {
    fn from(value: MoveView) -> Self {
        match value {
            MoveView::Play { color, point } => Self::Play(
                color.into(),
                go_core::Point {
                    x: point.x,
                    y: point.y,
                },
            ),
            MoveView::Pass { color } => Self::Pass(color.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BoardView {
    pub width: u8,
    pub height: u8,
    /// Row-major from the top-left: index i corresponds to
    /// `PointView { x: i % width, y: i / width }`. y = 0 is the top row:
    /// SGF row `a`, GTP row 19 on a 19x19 board, or row `height` on a rectangle.
    /// AnalysisView::ownership uses the same order.
    pub stones: Vec<Option<ColorView>>,
    /// Surviving stone's root-relative move number, including passes. Setup
    /// stones and empty points are null; preview stones use one-based PV indices.
    #[serde(default)]
    #[ts(as = "Option<Vec<Option<usize>>>", optional)]
    pub stone_move_numbers: Vec<Option<usize>>,
    /// Source node of each surviving recorded move; preview stones have no node.
    #[serde(default)]
    #[ts(as = "Option<Vec<Option<u32>>>", optional)]
    pub stone_node_ids: Vec<Option<u32>>,
    pub last_move: Option<MoveView>,
    pub to_play: ColorView,
    pub move_number: usize,
    pub captured_black: usize,
    pub captured_white: usize,
}

impl From<&GamePosition> for BoardView {
    fn from(position: &GamePosition) -> Self {
        Self {
            width: position.board.size().width,
            height: position.board.size().height,
            stones: position
                .board
                .stones()
                .iter()
                .map(|stone| stone.map(ColorView::from))
                .collect(),
            stone_move_numbers: position.stone_move_numbers.clone(),
            stone_node_ids: position
                .stone_node_ids
                .iter()
                .map(|node| node.map(|id| id.0))
                .collect(),
            last_move: position.moves.last().copied().map(MoveView::from),
            to_play: position.board.to_play().into(),
            move_number: position.moves.len(),
            captured_black: position.captured_by.black,
            captured_white: position.captured_by.white,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TreeView {
    /// Fetch variation_tree when either tree revision changes; navigation is independent.
    #[ts(type = "number")]
    pub structure_revision: u64,
    /// Fetch the tree when comment markers change, independently of structure.
    #[serde(default)]
    #[ts(as = "Option<f64>", optional)]
    pub comment_revision: u64,
    pub root: u32,
    pub current: u32,
    /// Root followed by each first child, including empty SGF nodes.
    pub main_line: Vec<u32>,
    /// Number of direct children of the current node, including the main line.
    pub variation_count: usize,
    /// First move after leaving the root's first-child main line. An empty
    /// branch starts numbering at its next move; nested branches keep this base.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub branch_start_move_number: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum WarningKindView {
    FirstValueUsed,
    InvalidValue { fallback: String },
    OutOfRange { fallback: String },
    UnknownRules { fallback: String },
    IgnoredNonRoot,
    IgnoredPlayer,
    NormalizedCase,
    KomiFromKo,
}

impl From<document::WarningKind> for WarningKindView {
    fn from(value: document::WarningKind) -> Self {
        match value {
            document::WarningKind::FirstValueUsed => Self::FirstValueUsed,
            document::WarningKind::InvalidValue { fallback } => Self::InvalidValue { fallback },
            document::WarningKind::OutOfRange { fallback } => Self::OutOfRange { fallback },
            document::WarningKind::UnknownRules { fallback } => Self::UnknownRules { fallback },
            document::WarningKind::IgnoredNonRoot => Self::IgnoredNonRoot,
            document::WarningKind::IgnoredPlayer => Self::IgnoredPlayer,
            document::WarningKind::NormalizedCase => Self::NormalizedCase,
            document::WarningKind::KomiFromKo => Self::KomiFromKo,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DocumentWarningView {
    pub node: u32,
    pub property: String,
    pub kind: WarningKindView,
}

impl From<document::DocumentWarning> for DocumentWarningView {
    fn from(value: document::DocumentWarning) -> Self {
        Self {
            node: value.node.0,
            property: value.property,
            kind: value.kind.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct DocumentView {
    /// Root SGF fields for the game information panel, independent of focus.
    pub game_info: Box<GameInfoView>,
    /// Invalid recorded positions remain navigable but have no invented board.
    pub board: Option<BoardView>,
    pub tree: TreeView,
    pub graph: GraphView,
    pub comment: Option<String>,
    // Current-node SGF marks, independent of the legality of its moves.
    #[serde(default)]
    #[ts(as = "Option<Vec<crate::MarkupView>>", optional)]
    pub markup: Vec<crate::MarkupView>,
    pub warnings: Vec<DocumentWarningView>,
    pub position_error: Option<String>,
    // File basename after opening or saving; non-file imports have no name.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub file_name: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub file_path: Option<String>,
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub modified: bool,
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub can_undo: bool,
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub can_redo: bool,
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub trial: bool,
}

impl DocumentView {
    /// The owning actor supplies its stable, application-wide tree version.
    pub fn from_document(
        document: &GameDocument,
        structure_revision: u64,
    ) -> Result<Self, DocError> {
        let current = document.node(document.current())?;
        let mut main_line = Vec::new();
        let mut next = Some(document.root());
        while let Some(id) = next {
            main_line.push(id.0);
            next = document.node(id)?.children().first().copied();
        }
        let (board, position_error) = match document.position_at(document.current()) {
            Ok(position) => (Some(BoardView::from(&position)), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let (graph, branch_start_move_number) = GraphView::from_document_with_branch(document)?;
        Ok(Self {
            file_name: None,
            file_path: None,
            modified: false,
            can_undo: document.can_undo(),
            can_redo: document.can_redo(),
            trial: false,
            game_info: Box::new(document.game_info()?.into()),
            board,
            tree: TreeView {
                structure_revision,
                comment_revision: 0,
                root: document.root().0,
                current: document.current().0,
                main_line,
                variation_count: current.children().len(),
                branch_start_move_number,
            },
            graph,
            comment: current.comment().map(str::to_owned),
            markup: document
                .markup_at(document.current())?
                .into_iter()
                .map(Into::into)
                .collect(),
            warnings: document.warnings().into_iter().map(Into::into).collect(),
            position_error,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CandidateView {
    /// GTP coordinate, including the literal `pass`.
    pub coordinate: String,
    /// Zero-based board point for rendering; None means pass.
    pub point: Option<PointView>,
    #[ts(type = "number")]
    pub visits: u64,
    pub winrate: f64,
    pub score_lead: f64,
    #[serde(default = "score_available")]
    #[ts(as = "Option<bool>", optional)]
    pub score_lead_available: bool,
    pub prior: f64,
    pub order: u32,
    /// Black-perspective ownership after this candidate, in board row order.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub ownership: Option<Vec<f64>>,
    /// Starts with the current player; colors alternate after every move/pass.
    pub pv: Vec<MoveView>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AnalysisSource {
    Cache,
    Live,
    Saved,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct AnalysisView {
    pub node_id: u32,
    #[ts(type = "number")]
    pub revision: u64,
    pub winrate: f64,
    pub score_lead: f64,
    #[ts(type = "number")]
    pub visits: u64,
    /// Recent live-search root visits per second; absent for cache/review results
    /// or until enough intermediate samples have arrived.
    #[ts(optional = nullable)]
    pub visits_per_second: Option<f64>,
    /// Best root evaluation for this node, document and engine fingerprint.
    /// AppCore merges this independently of the current result above, so the
    /// graph can retain deeper analysis while candidates and ownership update.
    pub summary: AnalysisSummary,
    pub candidates: Vec<CandidateView>,
    /// Black-perspective values in BoardView::stones order, row-major from
    /// the top-left: index i corresponds to
    /// `PointView { x: i % width, y: i / width }`. y = 0 is the top row:
    /// SGF row `a`, GTP row 19 on a 19x19 board, or row `height` on a rectangle.
    pub ownership: Option<Vec<f64>>,
    /// Raw to-play probabilities, row-major plus pass; illegal points are -1.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub policy: Option<Vec<f64>>,
    pub is_during_search: bool,
    pub source: AnalysisSource,
}

impl AnalysisView {
    /// Convert all engine coordinates using the focused document's dimensions
    /// and player. Invalid candidates or PV coordinates reject the whole view.
    pub fn from_snapshot(
        value: analysis::AnalysisSnapshot,
        size: go_core::BoardSize,
        to_play: go_core::Color,
    ) -> Result<Self, go_core::CoordinateError> {
        size.validate()?;
        let response = value.response;
        let mut candidates = Vec::with_capacity(response.move_infos.len());
        for candidate in response.move_infos {
            let point = match go_core::Point::from_gtp(&candidate.coordinate, size) {
                Ok(point) => Some(PointView {
                    x: point.x,
                    y: point.y,
                }),
                Err(go_core::CoordinateError::PassHasNoPoint) => None,
                Err(error) => return Err(error),
            };
            let mut color = to_play;
            let mut pv = Vec::with_capacity(candidate.pv.len());
            for coordinate in candidate.pv {
                pv.push(go_core::Move::from_gtp(color, &coordinate, size)?.into());
                color = color.opponent();
            }
            candidates.push(CandidateView {
                coordinate: candidate.coordinate,
                point,
                visits: candidate.visits,
                winrate: candidate.winrate,
                score_lead: candidate.score_lead,
                score_lead_available: true,
                prior: candidate.prior,
                order: candidate.order,
                ownership: candidate.ownership.filter(|values| {
                    values.len() == usize::from(size.width) * usize::from(size.height)
                }),
                pv,
            });
        }
        Ok(Self {
            node_id: value.node_id.0,
            revision: value.revision,
            winrate: response.root_info.winrate,
            score_lead: response.root_info.score_lead,
            visits: response.root_info.visits,
            visits_per_second: value.visits_per_second,
            summary: AnalysisSummary {
                winrate: response.root_info.winrate,
                score_lead: response.root_info.score_lead,
                score_lead_available: true,
                visits: response.root_info.visits,
            },
            candidates,
            ownership: response.ownership,
            policy: response.policy.filter(|values| {
                values.len() == usize::from(size.width) * usize::from(size.height) + 1
            }),
            is_during_search: response.is_during_search,
            source: match value.source {
                analysis::SnapshotSource::Cache => AnalysisSource::Cache,
                analysis::SnapshotSource::Live
                | analysis::SnapshotSource::Extras
                | analysis::SnapshotSource::PureNetwork => AnalysisSource::Live,
            },
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum EngineStatus {
    Starting,
    Ready {
        version: String,
    },
    Restarting {
        attempt: u32,
        reason: String,
    },
    Failed {
        reason: String,
        stderr_tail: Vec<String>,
    },
    Stopped,
}

impl From<engine_session::EngineStatus> for EngineStatus {
    fn from(value: engine_session::EngineStatus) -> Self {
        match value {
            engine_session::EngineStatus::Starting => Self::Starting,
            engine_session::EngineStatus::Ready { version } => Self::Ready { version },
            engine_session::EngineStatus::Restarting { attempt, reason } => {
                Self::Restarting { attempt, reason }
            }
            engine_session::EngineStatus::Failed {
                reason,
                stderr_tail,
            } => Self::Failed {
                reason,
                stderr_tail,
            },
            engine_session::EngineStatus::Stopped => Self::Stopped,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum AppEvent {
    MatchUpdated(crate::MatchView),
    GtpConsoleLine(crate::GtpConsoleLine),
    GameUpdated(crate::GameView),
    BatchAnalysisProgress(crate::BatchAnalysisStatus),
    UpdateDownloadProgress(crate::updates::UpdateDownloadStatus),
    AssetDownloadProgress(crate::AssetDownloadStatus),
    OpenFailed {
        path: String,
        reason: String,
    },
    AnalysisStateChanged(crate::AnalysisState),
    BenchmarkProgress(crate::BenchmarkStatus),
    BoardSyncStatusChanged(crate::BoardSyncStatus),
    DocumentChanged {
        #[ts(type = "number")]
        revision: u64,
        view: DocumentView,
    },
    AnalysisUpdated(AnalysisView),
    EngineStatusChanged(EngineStatus),
    WholeGameProgress(crate::WholeGameProgress),
}

#[cfg(test)]
mod tests;
