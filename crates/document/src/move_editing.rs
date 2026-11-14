//! Atomic move edits. Only affected nodes enter the reversible delta; imported
//! properties and unaffected branches keep their identity and relative order.

use std::collections::{BTreeMap, BTreeSet};

use go_core::{BoardSize, Color, IllegalMove, Move, Point};
use sgf::Property;

use crate::edit::{Change, Edit};
use crate::{DocError, DocumentNode, GameDocument, GamePosition, NodeId, semantics};

/// Provenance of a surviving ordinary move, not an initial/setup stone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoneOrigin {
    pub node: NodeId,
    pub move_number: usize,
    /// Java's click review starts before this move; jumping uses `node` itself.
    pub review_parent: Option<NodeId>,
}

#[derive(Debug, Clone)]
pub(crate) struct NodePatch {
    before: Option<DocumentNode>,
    after: DocumentNode,
}

/// A speculative delta, never a second document/history. Preparation can fail
/// without allocating an ID or touching the live tree and its undo/redo stacks.
struct PatchBuilder<'a> {
    document: &'a GameDocument,
    nodes: BTreeMap<NodeId, DocumentNode>,
    added: usize,
    normalization: BTreeSet<NodeId>,
    replayed: BTreeSet<NodeId>,
    redirects: BTreeMap<NodeId, NodeId>,
}

impl GameDocument {
    /// An isolated trial keeps the ancestor path and repetition history. Java
    /// tryPlay discards continuations at the starting point; temporary edits
    /// have their own initially empty undo history.
    pub fn trial_copy(&self) -> Result<Self, DocError> {
        self.position_at(self.current)?;
        let mut trial = self.clone();
        let mut pending = std::mem::take(&mut trial.arena[self.current.0 as usize].children);
        while let Some(id) = pending.pop() {
            let node = &mut trial.arena[id.0 as usize];
            node.live = false;
            pending.extend(node.children.iter().copied());
        }
        trial.undo.clear();
        trial.redo.clear();
        // This copy is a new document namespace at the application boundary.
        trial.revision = 0;
        trial.content_revision = 0;
        Ok(trial)
    }

    /// Reuses provenance computed during ordinary position replay. Captures
    /// and explicit setup clear it, even when setup writes the same color.
    pub fn stone_origin(&self, point: Point) -> Result<Option<StoneOrigin>, DocError> {
        let position = self.position_at(self.current)?;
        validate_point(self.current, point, position.board.size())?;
        let index =
            usize::from(point.y) * usize::from(position.board.size().width) + usize::from(point.x);
        let (Some(node), Some(move_number)) = (
            position.stone_node_ids[index],
            position.stone_move_numbers[index],
        ) else {
            return Ok(None);
        };
        Ok(Some(StoneOrigin {
            node,
            move_number,
            review_parent: self.node(node)?.parent,
        }))
    }

    pub(crate) fn prepare_insert_move(&self, mv: Move) -> Result<Edit, DocError> {
        let mut position = self.position_at(self.current)?;
        let properties = vec![move_property(self.current, mv, position.board.size())?];
        semantics::apply_node_with_rules(&mut position, self.current, &properties, true)?;
        let mut patch = PatchBuilder::new(self);
        let mut parent = patch.node(self.current)?;
        let old_children = parent.children.clone();
        let inserted = patch.add(self.current, properties)?;
        parent.children = vec![inserted];
        patch.put(parent);
        let mut node = patch.node(inserted)?;
        node.children = old_children.clone();
        patch.put(node);
        for child in &old_children {
            let mut node = patch.node(*child)?;
            node.parent = Some(inserted);
            patch.put(node);
        }
        patch.replay_continuations(old_children, position)?;
        patch.merge_replayed_branches(self.position_at(self.current)?.board.size())?;
        Ok(patch.finish(inserted))
    }

    pub(crate) fn prepare_stone_edit(
        &self,
        point: Point,
        destination: Option<Point>,
        switch: bool,
    ) -> Result<Option<Edit>, DocError> {
        let origin = self
            .stone_origin(point)?
            .ok_or(DocError::NoMoveAtPoint(point))?;
        let source = self.node(origin.node)?;
        let parent = source.parent;
        let mut position = if let Some(parent) = parent {
            self.position_at(parent)?
        } else {
            // A root may itself record a move. Removing that move must retain
            // the root, its metadata and its initial setup, not delete the tree.
            let initial: Vec<_> = source
                .properties
                .iter()
                .filter(|property| !matches!(property.identifier.as_str(), "B" | "W"))
                .cloned()
                .collect();
            semantics::replay([(source.id, initial.as_slice())])?
        };
        let size = position.board.size();
        let Some(Move::Play(color, _)) =
            semantics::recorded_move(source.id, &source.properties, size)?
        else {
            return Err(DocError::NoMoveAtPoint(point));
        };
        if let Some(destination) = destination {
            validate_point(self.current, destination, size)?;
            if destination == point {
                return Ok(None);
            }
            // Java DraggedReleased rejects a destination occupied on the board
            // the user sees, even if it was empty before the edited move.
            if self
                .position_at(self.current)?
                .board
                .stone_at(destination)
                .is_some()
            {
                return Err(DocError::IllegalMove {
                    node: self.current,
                    reason: IllegalMove::Occupied { point: destination },
                });
            }
        }
        let replacement = if switch {
            Some(Move::Play(color.opponent(), point))
        } else {
            destination.map(|destination| Move::Play(color, destination))
        };
        let mut node = source.clone();
        node.properties = replace_move(&node.properties, replacement, source.id, size)?;
        // A newly requested illegal move is an error, not silent data loss.
        semantics::apply_node_with_rules(&mut position, node.id, &node.properties, true)?;
        let descendants = node.children.clone();
        let mut patch = PatchBuilder::new(self);
        patch.put(node.clone());
        patch.replayed.insert(node.id);
        patch.normalization.extend(parent);
        if parent.is_some() && replacement.is_none() && node.properties.is_empty() {
            patch.splice_out(node.id)?;
        }
        patch.replay_continuations(descendants, position)?;
        patch.merge_replayed_branches(size)?;
        let mut focus = self.current;
        while !patch.node(focus)?.live {
            focus = if let Some(target) = patch.redirects.get(&focus) {
                *target
            } else {
                patch.node(focus)?.parent.ok_or(DocError::RootOperation)?
            };
        }
        Ok(Some(patch.finish(focus)))
    }

    pub(crate) fn prepare_add_variation(&self, moves: &[Move]) -> Result<Option<Edit>, DocError> {
        let mut ancestor = self.current;
        let mut side_branch = false;
        while let Some(parent) = self.node(ancestor)?.parent {
            side_branch |= self.node(parent)?.children.first() != Some(&ancestor);
            ancestor = parent;
        }
        // LizzieFrame routes a non-main leaf through playCurrentVariation,
        // which extends the branch and leaves focus at its new final move.
        let focus_at_end = side_branch && self.node(self.current)?.children.is_empty();
        self.prepare_play_sequence(moves, focus_at_end)
    }

    pub(crate) fn prepare_play_sequence(
        &self,
        moves: &[Move],
        focus_at_end: bool,
    ) -> Result<Option<Edit>, DocError> {
        if moves.is_empty() {
            return Ok(None);
        }
        // Validate the entire supplied PV before committing. It is a command,
        // unlike the partial-result preview query; passes are real SGF moves.
        let preview = self.preview_variation(self.current, moves)?;
        if let Some(error) = preview.stop_reason {
            return Err(error);
        }
        let mut patch = PatchBuilder::new(self);
        let mut parent = self.current;
        for &mv in moves {
            let properties = vec![move_property(parent, mv, preview.position.board.size())?];
            let child = patch.add(parent, properties)?;
            let mut node = patch.node(parent)?;
            node.children.push(child);
            patch.put(node);
            parent = child;
        }
        // The usual BoardRenderer route uses newBranch=true for the first move,
        // so identical prefixes form a separate variation and toBranchTop
        // restores its parent. No UI-only dummy pass is inserted here.
        Ok(Some(patch.finish(if focus_at_end {
            parent
        } else {
            self.current
        })))
    }

    pub(crate) fn execute_node_patches(&mut self, patches: &[NodePatch], forward: bool) {
        for patch in patches {
            let id = patch.after.id.0 as usize;
            let node = if forward {
                patch.after.clone()
            } else if let Some(before) = &patch.before {
                before.clone()
            } else {
                let mut tombstone = patch.after.clone();
                tombstone.live = false;
                tombstone
            };
            if id == self.arena.len() {
                self.arena.push(node);
            } else {
                self.arena[id] = node;
            }
        }
    }
}

impl<'a> PatchBuilder<'a> {
    fn new(document: &'a GameDocument) -> Self {
        Self {
            document,
            nodes: BTreeMap::new(),
            added: 0,
            normalization: BTreeSet::new(),
            replayed: BTreeSet::new(),
            redirects: BTreeMap::new(),
        }
    }

    fn node(&self, id: NodeId) -> Result<DocumentNode, DocError> {
        self.nodes
            .get(&id)
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| self.document.node(id).cloned())
    }

    fn put(&mut self, node: DocumentNode) {
        self.normalization.insert(node.id);
        self.nodes.insert(node.id, node);
    }

    fn add(&mut self, parent: NodeId, properties: Vec<Property>) -> Result<NodeId, DocError> {
        let index = self
            .document
            .arena
            .len()
            .checked_add(self.added)
            .ok_or(DocError::NodeLimit)?;
        let id = NodeId(u32::try_from(index).map_err(|_| DocError::NodeLimit)?);
        self.added += 1;
        self.replayed.insert(id);
        self.put(DocumentNode {
            id,
            parent: Some(parent),
            children: Vec::new(),
            properties,
            live: true,
        });
        Ok(id)
    }

    fn splice_out(&mut self, id: NodeId) -> Result<(), DocError> {
        let mut node = self.node(id)?;
        let parent_id = node.parent.ok_or(DocError::RootOperation)?;
        let mut parent = self.node(parent_id)?;
        let index = parent
            .children
            .iter()
            .position(|child| *child == id)
            .ok_or(DocError::MissingNode(id))?;
        parent
            .children
            .splice(index..=index, node.children.iter().copied());
        for &child in &node.children {
            let mut child = self.node(child)?;
            child.parent = Some(parent_id);
            self.put(child);
        }
        node.live = false;
        self.put(node);
        self.put(parent);
        Ok(())
    }

    fn replay_continuations(
        &mut self,
        children: Vec<NodeId>,
        position: GamePosition,
    ) -> Result<(), DocError> {
        let mut pending: Vec<_> = children
            .into_iter()
            .rev()
            .map(|id| (id, position.clone()))
            .collect();
        while let Some((id, before)) = pending.pop() {
            let mut node = self.node(id)?;
            self.replayed.insert(id);
            self.normalization.insert(id);
            self.normalization.extend(node.parent);
            let mut after = before.clone();
            match semantics::apply_node_with_rules(&mut after, id, &node.properties, true) {
                Ok(()) => {}
                Err(DocError::IllegalMove { .. }) => {
                    // Java Board.place returns without moving on an illegal
                    // continuation, and placeLinkedList still visits children.
                    node.properties =
                        replace_move(&node.properties, None, id, before.board.size())?;
                    after = before;
                    semantics::apply_node_with_rules(&mut after, id, &node.properties, true)?;
                    self.put(node.clone());
                    if node.properties.is_empty() {
                        self.splice_out(id)?;
                    }
                }
                Err(error) => return Err(error),
            }
            pending.extend(
                node.children
                    .into_iter()
                    .rev()
                    .map(|child| (child, after.clone())),
            );
        }
        Ok(())
    }

    /// Board.place/addOrGoto reuses existing same-color moves during subtree
    /// replay. Untouched siblings already existed when Java removed the old
    /// subtree, so they win over a moved/replayed sibling even if it now appears
    /// earlier in child order. Non-move properties prevent merging: their data
    /// must not disappear as a side effect of editing a different stone.
    fn merge_replayed_branches(&mut self, size: BoardSize) -> Result<(), DocError> {
        let mut pending = std::mem::take(&mut self.normalization);
        while let Some(parent_id) = pending.pop_first() {
            let mut parent = self.node(parent_id)?;
            if !parent.live {
                continue;
            }
            let mut groups: Vec<(Move, Vec<NodeId>)> = Vec::new();
            for &child in &parent.children {
                let node = self.node(child)?;
                if let Some(mv) = pure_move(&node, size) {
                    if let Some((_, ids)) = groups.iter_mut().find(|(known, _)| *known == mv) {
                        ids.push(child);
                    } else {
                        groups.push((mv, vec![child]));
                    }
                }
            }
            for (_, ids) in groups.into_iter().filter(|(_, ids)| ids.len() > 1) {
                if !ids.iter().any(|id| self.replayed.contains(id)) {
                    continue;
                }
                let target = ids
                    .iter()
                    .copied()
                    .find(|id| !self.replayed.contains(id))
                    .unwrap_or(ids[0]);
                let mut kept = self.node(target)?;
                let removed_ids: Vec<_> = ids
                    .into_iter()
                    .filter(|id| *id != target && self.replayed.contains(id))
                    .collect();
                for id in removed_ids {
                    let mut removed = self.node(id)?;
                    for &child in &removed.children {
                        let mut child_node = self.node(child)?;
                        child_node.parent = Some(target);
                        self.nodes.insert(child, child_node);
                    }
                    kept.children.extend(removed.children.iter().copied());
                    removed.live = false;
                    parent.children.retain(|child| *child != id);
                    self.redirects.insert(id, target);
                    self.nodes.insert(id, removed);
                }
                self.nodes.insert(target, kept);
                pending.insert(target);
            }
            self.nodes.insert(parent_id, parent);
        }
        Ok(())
    }

    fn finish(self, after: NodeId) -> Edit {
        let patches = self
            .nodes
            .into_iter()
            .filter_map(|(id, after)| {
                let before = self.document.arena.get(id.0 as usize).cloned();
                (before.as_ref() != Some(&after)).then_some(NodePatch { before, after })
            })
            .collect();
        Edit {
            before: self.document.current,
            after,
            change: Change::Nodes(patches),
        }
    }
}

fn pure_move(node: &DocumentNode, size: BoardSize) -> Option<Move> {
    if node.properties.len() != 1 {
        return None;
    }
    // An untouched imported sibling may be malformed. It cannot be matched,
    // but must not prevent editing an otherwise valid branch.
    semantics::recorded_move(node.id, &node.properties, size)
        .ok()
        .flatten()
}

fn validate_point(node: NodeId, point: Point, size: BoardSize) -> Result<(), DocError> {
    point
        .to_sgf(size)
        .map(|_| ())
        .map_err(|_| DocError::IllegalMove {
            node,
            reason: IllegalMove::OutOfBounds { point, size },
        })
}

fn move_property(node: NodeId, mv: Move, size: BoardSize) -> Result<Property, DocError> {
    let value = mv.to_sgf(size).map_err(|_| DocError::InvalidProperty {
        node,
        property: "move".into(),
        value: format!("{mv:?}"),
    })?;
    Ok(Property {
        identifier: match mv.color() {
            Color::Black => "B",
            Color::White => "W",
        }
        .into(),
        values: vec![value],
    })
}

fn replace_move(
    properties: &[Property],
    mv: Option<Move>,
    node: NodeId,
    size: BoardSize,
) -> Result<Vec<Property>, DocError> {
    let mut replacement = mv.map(|mv| move_property(node, mv, size)).transpose()?;
    let mut updated = Vec::with_capacity(properties.len());
    for property in properties {
        if matches!(property.identifier.as_str(), "B" | "W") {
            if let Some(replacement) = replacement.take() {
                updated.push(replacement);
            }
        } else {
            updated.push(property.clone());
        }
    }
    if let Some(replacement) = replacement {
        updated.push(replacement);
    }
    Ok(updated)
}

#[cfg(test)]
mod tests;
