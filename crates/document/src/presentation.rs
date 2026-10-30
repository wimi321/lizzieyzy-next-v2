//! Read-only game information and complete ordered tree, independent of replay.

use go_core::Move;

use crate::{DocError, GameDocument, NodeId, metadata, semantics};

/// Root SGF information for display. Text uses the first property's first value
/// verbatim: absent fields are None, explicitly empty values remain Some("").
#[derive(Debug, Clone, PartialEq)]
pub struct GameInfo {
    pub black_name: Option<String>,
    pub black_rank: Option<String>,
    pub white_name: Option<String>,
    pub white_rank: Option<String>,
    /// Effective komi, using exactly the same tolerant reading as replay.
    pub komi: f64,
    /// Original RU text, rather than the effective fallback rule preset.
    pub rules: Option<String>,
    /// None if HA is absent; present malformed HA uses replay's fallback of 0.
    pub handicap: Option<u32>,
    pub result: Option<String>,
    pub date: Option<String>,
    pub event: Option<String>,
}

/// One live tree node. The first child is its main continuation. Node IDs are
/// stable within a document and are not array indexes in the returned view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariationNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// Number of move-bearing nodes on the root-to-node path, including passes
    /// and root moves. Empty/setup nodes do not increment this. A malformed
    /// move-bearing node still counts as one, matching the graph's convention.
    pub move_number: usize,
    /// None for an empty/setup node, or a malformed move (see move_error).
    pub mv: Option<Move>,
    /// Only SGF move decoding errors, not a replay/legality verdict. For example,
    /// an occupied recorded point still has a move here; position_at checks it.
    pub move_error: Option<DocError>,
}

impl GameDocument {
    /// Metadata remains readable even if a recorded move/setup is invalid.
    pub fn game_info(&self) -> Result<GameInfo, DocError> {
        let root = self.node(self.root())?;
        let properties = root.properties();
        let metadata = metadata::root_metadata(root.id(), properties);
        let text = |name: &str| {
            properties
                .iter()
                .find(|property| property.identifier == name)
                .map(|property| property.values.first().cloned().unwrap_or_default())
        };
        Ok(GameInfo {
            black_name: text("PB"),
            black_rank: text("BR"),
            white_name: text("PW"),
            white_rank: text("WR"),
            komi: metadata.komi,
            rules: metadata.rules_raw,
            handicap: text("HA").map(|_| metadata.handicap),
            result: text("RE"),
            date: text("DT"),
            event: text("EV"),
        })
    }

    /// All live nodes in ordered depth-first traversal, including root and
    /// empty nodes. Iterative O(nodes + properties) traversal avoids replaying
    /// every path, and malformed moves do not hide descendants or siblings.
    pub fn variation_tree(&self) -> Result<Vec<VariationNode>, DocError> {
        let root = self.node(self.root())?;
        let size = metadata::root_metadata(root.id(), root.properties()).size;
        let mut pending = vec![(root.id(), 0)];
        let mut nodes = Vec::new();
        while let Some((id, parent_move_number)) = pending.pop() {
            let node = self.node(id)?;
            let has_move = node
                .properties()
                .iter()
                .any(|property| matches!(property.identifier.as_str(), "B" | "W"));
            let move_number = parent_move_number + usize::from(has_move);
            let (mv, move_error) = match semantics::recorded_move(id, node.properties(), size) {
                Ok(mv) => (mv, None),
                Err(error) => (None, Some(error)),
            };
            pending.extend(node.children().iter().rev().map(|id| (*id, move_number)));
            nodes.push(VariationNode {
                id,
                parent: node.parent(),
                children: node.children().to_vec(),
                move_number,
                mv,
                move_error,
            });
        }
        Ok(nodes)
    }
}

#[cfg(test)]
mod tests;
