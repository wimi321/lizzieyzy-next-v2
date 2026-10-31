//! Serializable rendering data. Go rules and analysis remain in the core crates.

use document::{DocError, GameDocument, GamePosition};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{AnalysisSummary, GameInfoView, GraphView};

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
    /// Fetch variation_tree only when this changes; independent of navigation.
    #[ts(type = "number")]
    pub structure_revision: u64,
    pub root: u32,
    pub current: u32,
    /// Root followed by each first child, including empty SGF nodes.
    pub main_line: Vec<u32>,
    /// Number of direct children of the current node, including the main line.
    pub variation_count: usize,
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
    pub warnings: Vec<DocumentWarningView>,
    pub position_error: Option<String>,
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
        Ok(Self {
            game_info: Box::new(document.game_info()?.into()),
            board,
            tree: TreeView {
                structure_revision,
                root: document.root().0,
                current: document.current().0,
                main_line,
                variation_count: current.children().len(),
            },
            graph: GraphView::from_document(document)?,
            comment: current.comment().map(str::to_owned),
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
    pub prior: f64,
    pub order: u32,
    /// Starts with the current player; colors alternate after every move/pass.
    pub pv: Vec<MoveView>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AnalysisSource {
    Cache,
    Live,
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
                prior: candidate.prior,
                order: candidate.order,
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
                visits: response.root_info.visits,
            },
            candidates,
            ownership: response.ownership,
            is_during_search: response.is_during_search,
            source: match value.source {
                analysis::SnapshotSource::Cache => AnalysisSource::Cache,
                analysis::SnapshotSource::Live => AnalysisSource::Live,
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
