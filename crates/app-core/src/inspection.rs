//! Read-only data for the game information panel, variation tree and PV hover.

use document::{DocError, GameDocument, NodeId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{AppCore, AppError, BoardView, ColorView, MoveView, PointView, actor};

/// Root SGF information. Missing fields are null, explicit empty text stays "".
/// Rules are the original RU text; this is not the effective engine preset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct GameInfoView {
    /// PB / BR, without rank or name guessing.
    pub black_name: Option<String>,
    pub black_rank: Option<String>,
    /// PW / WR, without rank or name guessing.
    pub white_name: Option<String>,
    pub white_rank: Option<String>,
    /// Effective komi, using the document's existing tolerant KM/KO rules.
    pub komi: f64,
    pub rules: Option<String>,
    /// Null when HA is absent; a present but invalid HA uses document fallback 0.
    pub handicap: Option<u32>,
    /// RE, DT and EV are preserved text, not parsed dates or results.
    pub result: Option<String>,
    pub date: Option<String>,
    pub event: Option<String>,
}

impl From<document::GameInfo> for GameInfoView {
    fn from(info: document::GameInfo) -> Self {
        Self {
            black_name: info.black_name,
            black_rank: info.black_rank,
            white_name: info.white_name,
            white_rank: info.white_rank,
            komi: info.komi,
            rules: info.rules,
            handicap: info.handicap,
            result: info.result,
            date: info.date,
            event: info.event,
        }
    }
}

/// One live node, including the root and non-move SGF nodes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VariationNodeView {
    pub id: u32,
    pub parent: Option<u32>,
    /// Ordered child IDs. The first child continues the main variation.
    pub children: Vec<u32>,
    /// Recorded moves on the root-to-node path, including passes and root moves.
    /// Empty/setup nodes add no move. A malformed B/W node still counts once.
    pub move_number: usize,
    /// Null for a node without a move, or an unparseable move (see move_error).
    /// Pass is an explicit MoveView::Pass, never null.
    pub mv: Option<MoveView>,
    /// Local move syntax/coordinate error only; not a full legality assessment.
    pub move_error: Option<String>,
}

/// Fetch when DocumentView.tree.structure_revision changes. Navigation and
/// comments do not change this version. Opening a game always changes it, even
/// if node IDs and structure match the previous game. Discard an asynchronous
/// reply whose revision differs from the latest structure_revision.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VariationTreeView {
    #[ts(type = "number")]
    pub revision: u64,
    pub root: u32,
    /// Depth-first, parent before children, in SGF child order. No tombstones.
    pub nodes: Vec<VariationNodeView>,
}

impl VariationTreeView {
    pub(crate) fn refresh(
        &mut self,
        document: &GameDocument,
        revision: u64,
    ) -> Result<(), DocError> {
        let nodes = document
            .variation_tree()?
            .into_iter()
            .map(|node| VariationNodeView {
                id: node.id.0,
                parent: node.parent.map(|id| id.0),
                children: node.children.into_iter().map(|id| id.0).collect(),
                move_number: node.move_number,
                mv: node.mv.map(Into::into),
                move_error: node.move_error.map(|error| error.to_string()),
            })
            .collect::<Vec<_>>();
        if self.root != document.root().0 || self.nodes != nodes {
            self.revision = revision;
            self.root = document.root().0;
            self.nodes = nodes;
        }
        Ok(())
    }
}

/// A preview stone still present on the final board. Captured stones have no
/// label; replaying onto a freed point gives that point the newer move number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PreviewStoneView {
    pub point: PointView,
    pub color: ColorView,
    /// One-based PV index (not the game's move number); passes consume an index.
    pub move_number: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VariationPreviewView {
    /// Document revision at execution, for rejecting stale hover responses.
    /// Compare with the document revision captured when requesting this PV.
    #[ts(type = "number")]
    pub revision: u64,
    pub node_id: u32,
    /// Final board after the legal prefix, including captures and the next player.
    pub board: BoardView,
    /// Accepted PV prefix, including passes. Empty if the first move was illegal.
    pub applied_moves: Vec<MoveView>,
    /// Labels for surviving preview stones only, in PV order.
    pub stones: Vec<PreviewStoneView>,
    /// First rejected move's error; null when the entire input was accepted.
    pub stop_reason: Option<String>,
}

impl VariationPreviewView {
    pub(crate) fn from_document(
        document: &GameDocument,
        revision: u64,
        node_id: u32,
        moves: &[go_core::Move],
    ) -> Result<Self, DocError> {
        let preview = document.preview_variation(NodeId(node_id), moves)?;
        Ok(Self {
            revision,
            node_id,
            board: BoardView::from(&preview.position),
            applied_moves: preview.applied_moves.into_iter().map(Into::into).collect(),
            stones: preview
                .stones
                .into_iter()
                .map(|stone| PreviewStoneView {
                    point: PointView {
                        x: stone.point.x,
                        y: stone.point.y,
                    },
                    color: stone.color.into(),
                    move_number: stone.move_number,
                })
                .collect(),
            stop_reason: preview.stop_reason.map(|error| error.to_string()),
        })
    }
}

impl AppCore {
    /// Read the complete current tree without changing focus or analysis.
    pub async fn variation_tree(&self) -> Result<VariationTreeView, AppError> {
        let (reply, response) = tokio::sync::oneshot::channel();
        self.commands
            .send(actor::Command::VariationTree(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }

    /// Preview from an existing node (normally the UI's current node). This
    /// never navigates, edits history, publishes events or requests analysis.
    /// Invalid base nodes fail; invalid PV moves return the accepted prefix.
    pub async fn preview_variation(
        &self,
        node_id: u32,
        moves: Vec<go_core::Move>,
    ) -> Result<VariationPreviewView, AppError> {
        let (reply, response) = tokio::sync::oneshot::channel();
        self.commands
            .send(actor::Command::PreviewVariation(node_id, moves, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}
