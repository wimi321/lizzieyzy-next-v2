//! The comparison slot shares the primary codec, but preserves untouched raw
//! properties when the caller has no new comparison result to export.

use std::collections::BTreeMap;

use sgf::{GameTree, Property};

use super::{JAVA_MAX_VISITS, LzError, SavedAnalysis, format_with_warnings, import, is_comparison};
use crate::{DocError, DocumentWarning, GameDocument, NodeId, WarningKind};

impl GameDocument {
    /// Reads the last valid LZ2/LZOP2 payload at this node. The primary slot is
    /// never a fallback for a missing or malformed comparison payload.
    pub fn saved_comparison_analysis(&self, id: NodeId) -> Result<Option<SavedAnalysis>, DocError> {
        let node = self.node(id)?;
        if !node.properties.iter().any(|p| is_comparison(&p.identifier)) {
            return Ok(None);
        }
        let position = self.position_at(id)?;
        Ok(import::at_position_for_slot(node, &position, is_comparison))
    }

    /// Raw presence is sufficient for offering the comparison layout, even if
    /// malformed payloads can only be preserved and diagnosed, not displayed.
    pub fn has_comparison_analysis(&self) -> bool {
        self.arena
            .iter()
            .any(|node| node.live && node.properties.iter().any(|p| is_comparison(&p.identifier)))
    }

    /// Exports both independent slots. `primary` keeps the existing complete
    /// valid-result semantics; `comparison` contains this session's results.
    /// Nodes absent from `comparison` retain their original secondary data.
    /// A new result can replace the saved secondary payload only if deeper.
    pub fn to_sgf_with_analyses_and_warnings(
        &self,
        primary: &BTreeMap<NodeId, SavedAnalysis>,
        comparison: &BTreeMap<NodeId, SavedAnalysis>,
    ) -> Result<(GameTree, Vec<DocumentWarning>), LzError> {
        let (mut tree, mut warnings) = self.to_sgf_with_analysis_and_warnings(primary)?;
        let mut replacements = BTreeMap::new();
        for (&id, analysis) in comparison {
            let position = self.position_at(id).map_err(|e| LzError(e.to_string()))?;
            let original = self
                .saved_comparison_analysis(id)
                .map_err(|e| LzError(e.to_string()))?;
            if original.is_some_and(|saved| saved.visits >= analysis.visits) {
                continue;
            }
            let (payload, clamped) =
                format_with_warnings(analysis, position.board.size(), position.board.to_play())?;
            replacements.insert(id, (payload, position.board.size()));
            if clamped {
                warnings.push(DocumentWarning {
                    node: id,
                    property: "LZ2".into(),
                    kind: WarningKind::OutOfRange {
                        fallback: JAVA_MAX_VISITS.to_string(),
                    },
                });
            }
        }
        // The exported SGF walks active nodes in document preorder, including
        // tombstone gaps and edited variations; IDs are not array indices here.
        let mut ids = self
            .variation_tree()
            .map_err(|e| LzError(e.to_string()))?
            .into_iter();
        let mut pending = vec![&mut tree];
        while let Some(branch) = pending.pop() {
            for node in &mut branch.sequence {
                if let Some(entry) = ids.next()
                    && let Some((payload, size)) = replacements.remove(&entry.id)
                {
                    for property in &mut node.properties {
                        if is_comparison(&property.identifier) {
                            property.values.retain(|value| {
                                super::parse(value, size, go_core::Color::Black).is_err()
                            });
                        }
                    }
                    node.properties
                        .retain(|p| !is_comparison(&p.identifier) || !p.values.is_empty());
                    node.properties.push(Property {
                        identifier: "LZ2".into(),
                        values: vec![payload],
                    });
                }
            }
            pending.extend(branch.children.iter_mut().rev());
        }
        Ok((tree, warnings))
    }
}
