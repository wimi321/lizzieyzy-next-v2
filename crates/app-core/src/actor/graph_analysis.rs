//! Validation of graph summaries is tied to content, never to focus or visits.

use super::Actor;
use document::{NodeId, Revision};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct State {
    content_revision: Option<Revision>,
    pub valid_nodes: HashSet<NodeId>,
    #[cfg(test)]
    recomputations: usize,
    #[cfg(test)]
    replays: usize,
}

impl State {
    /// All entries were validated while restoring/importing this document.
    pub fn validated_content(&mut self, revision: Revision) {
        self.content_revision = Some(revision);
    }

    pub fn clear(&mut self) {
        self.content_revision = None;
        self.valid_nodes.clear();
    }
}

impl Actor {
    pub(super) fn refresh_graph_analysis(&mut self) {
        let content_revision = self.document.content_revision();
        if self.graph_analysis.content_revision == Some(content_revision) {
            return;
        }
        #[cfg(test)]
        {
            self.graph_analysis.recomputations += 1;
        }
        self.graph_analysis.valid_nodes.clear();
        // Retain the results themselves, including deleted nodes, so undo can
        // recover them. Only their visibility needs revalidation after an edit.
        for (&node, stored) in &self.review.results {
            #[cfg(test)]
            {
                self.graph_analysis.replays += 1;
            }
            if self
                .document
                .position_at(node)
                .is_ok_and(|position| position.analysis_key() == stored.key)
            {
                self.graph_analysis.valid_nodes.insert(node);
            }
        }
        self.graph_analysis.content_revision = Some(content_revision);
    }
}

#[cfg(test)]
mod tests;
