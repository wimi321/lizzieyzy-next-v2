//! Graph rendering data for the line through the current document node.

use document::{DocError, GameDocument};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Root evaluation in Black's perspective, copied from a published analysis.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct AnalysisSummary {
    pub winrate: f64,
    pub score_lead: f64,
    #[ts(type = "number")]
    pub visits: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct GraphPointView {
    pub node_id: u32,
    /// Recorded moves from the root, including passes, not tree depth.
    pub move_number: usize,
    pub summary: Option<AnalysisSummary>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct GraphView {
    /// Root to current, then first children through the end of that variation.
    pub points: Vec<GraphPointView>,
    pub current_index: usize,
}

impl GraphView {
    pub(crate) fn from_document(document: &GameDocument) -> Result<Self, DocError> {
        let mut line = Vec::new();
        let mut next = Some(document.current());
        while let Some(id) = next {
            line.push(id);
            next = document.node(id)?.parent();
        }
        line.reverse();
        let current_index = line.len() - 1;
        next = document
            .node(document.current())?
            .children()
            .first()
            .copied();
        while let Some(id) = next {
            line.push(id);
            next = document.node(id)?.children().first().copied();
        }

        let mut points = Vec::with_capacity(line.len());
        let mut move_number = 0;
        for id in line {
            // On valid positions this is exactly GamePosition::moves.len(),
            // as used by BoardView. Empty/setup/PL nodes do not add or reset a
            // move. Invalid records remain navigable: count a B/W node once
            // without claiming that its recorded move is legal.
            if document
                .node(id)?
                .properties()
                .iter()
                .any(|property| matches!(property.identifier.as_str(), "B" | "W"))
            {
                move_number += 1;
            }
            points.push(GraphPointView {
                node_id: id.0,
                move_number,
                summary: None,
            });
        }
        Ok(Self {
            points,
            current_index,
        })
    }
}
