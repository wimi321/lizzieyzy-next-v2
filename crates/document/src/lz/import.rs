//! Sparse, iterative replay of the ancestor paths that carry saved analysis.

use super::{SavedAnalysis, is_primary, parse};
use crate::{DocError, DocumentNode, GameDocument, GamePosition, NodeId, semantics};

pub(super) fn at_position(node: &DocumentNode, position: &GamePosition) -> Option<SavedAnalysis> {
    node.properties
        .iter()
        .filter(|property| is_primary(&property.identifier))
        .flat_map(|property| &property.values)
        .filter_map(|value| parse(value, position.board.size(), position.board.to_play()).ok())
        .next_back()
}

#[derive(Default)]
struct Work {
    #[cfg(test)]
    root_replays: usize,
    #[cfg(test)]
    applied: Vec<NodeId>,
}

impl GameDocument {
    /// Visits primary file analysis with its already-computed position. Scan
    /// properties first, then replay only paths leading to LZ/LZOP nodes. Shared
    /// ancestors are applied once; no position is computed for an irrelevant
    /// tail/branch or a document without primary analysis. Invalid positions
    /// skip their descendants, just as position_at would reject that path.
    pub fn visit_saved_analysis<E: From<DocError>>(
        &self,
        visitor: impl FnMut(NodeId, &GamePosition, SavedAnalysis) -> Result<(), E>,
    ) -> Result<(), E> {
        self.visit_saved_analysis_inner(visitor).map(|_| ())
    }

    fn visit_saved_analysis_inner<E: From<DocError>>(
        &self,
        mut visitor: impl FnMut(NodeId, &GamePosition, SavedAnalysis) -> Result<(), E>,
    ) -> Result<Work, E> {
        let mut work = Work::default();
        let mut order = Vec::new();
        let mut pending = vec![self.root];
        while let Some(id) = pending.pop() {
            let node = self.node(id)?;
            order.push(id);
            pending.extend(node.children.iter().rev().copied());
        }
        let mut needed = vec![false; self.arena.len()];
        for &id in order.iter().rev() {
            let node = self.node(id)?;
            needed[id.0 as usize] |= node
                .properties
                .iter()
                .any(|property| is_primary(&property.identifier));
            if needed[id.0 as usize]
                && let Some(parent) = node.parent
            {
                needed[parent.0 as usize] = true;
            }
        }
        if !needed[self.root.0 as usize] {
            return Ok(work);
        }
        let root = self.node(self.root)?;
        #[cfg(test)]
        {
            work.root_replays += 1;
            work.applied.push(root.id);
        }
        let Ok(position) = semantics::replay([(root.id, root.properties.as_slice())]) else {
            return Ok(work);
        };
        let mut pending = vec![(root.id, position)];
        while let Some((mut id, mut position)) = pending.pop() {
            loop {
                let node = self.node(id)?;
                if let Some(saved) = at_position(node, &position) {
                    visitor(id, &position, saved)?;
                }
                let mut children = node
                    .children
                    .iter()
                    .copied()
                    .filter(|id| needed[id.0 as usize]);
                let Some(first) = children.next() else {
                    break;
                };
                // Only forks clone the replay state. A long single-child line
                // advances in place instead of cloning its entire move history.
                for sibling in children.rev() {
                    let mut branch = position.clone();
                    if self
                        .apply_saved_node(&mut branch, sibling, &mut work)
                        .is_ok()
                    {
                        pending.push((sibling, branch));
                    }
                }
                if self
                    .apply_saved_node(&mut position, first, &mut work)
                    .is_err()
                {
                    break;
                }
                id = first;
            }
        }
        Ok(work)
    }

    fn apply_saved_node(
        &self,
        position: &mut GamePosition,
        id: NodeId,
        _work: &mut Work,
    ) -> Result<(), DocError> {
        #[cfg(test)]
        _work.applied.push(id);
        semantics::apply_node(position, id, self.node(id)?.properties())
    }
}

#[cfg(test)]
mod tests;
