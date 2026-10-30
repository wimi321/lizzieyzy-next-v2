use sgf::{GameTree, Node};

use crate::edit::Edit;
use crate::{
    DocError, DocumentNode, DocumentSnapshot, DocumentWarning, GamePosition, NodeId, Revision,
    semantics,
};

/// An owned synchronous arena. Only commands mutate live content. Detached
/// nodes stay in the arena so undo and future allocation cannot recycle IDs.
#[derive(Debug, Clone)]
pub struct GameDocument {
    pub(crate) arena: Vec<DocumentNode>,
    pub(crate) root: NodeId,
    pub(crate) current: NodeId,
    pub(crate) revision: Revision,
    pub(crate) undo: Vec<Edit>,
    pub(crate) redo: Vec<Edit>,
}

impl Default for GameDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl GameDocument {
    /// Empty 19x19 game: Chinese rules, 7.5 points komi, Black to play.
    pub fn new() -> Self {
        let root = NodeId(0);
        Self {
            arena: vec![DocumentNode {
                id: root,
                parent: None,
                children: Vec::new(),
                properties: Vec::new(),
                live: true,
            }],
            root,
            current: root,
            revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Imports one game. Semantic errors remain inspectable at their nodes;
    /// position_at/node_error report them without discarding SGF data.
    pub fn from_sgf(game: &GameTree) -> Result<Self, DocError> {
        let mut document = Self::new();
        document.arena.clear();
        let mut pending = vec![(game, None)];
        while let Some((tree, mut parent)) = pending.pop() {
            if tree.sequence.is_empty() {
                return Err(DocError::EmptyTree);
            }
            for node in &tree.sequence {
                let id = document.allocate(parent, node.properties.clone())?;
                if let Some(parent) = parent {
                    document.arena[parent.0 as usize].children.push(id);
                }
                parent = Some(id);
            }
            pending.extend(tree.children.iter().rev().map(|tree| (tree, parent)));
        }
        Ok(document)
    }

    /// Flattens single-child continuations into SGF sequences. All node
    /// properties and variation order are retained; no recursive traversal.
    pub fn to_sgf(&self) -> GameTree {
        let mut built: Vec<Option<GameTree>> = vec![None; self.arena.len()];
        let mut pending = vec![(self.root, false)];
        while let Some((start, visited)) = pending.pop() {
            let mut end = start;
            let mut sequence = Vec::new();
            loop {
                let node = &self.arena[end.0 as usize];
                if visited {
                    sequence.push(Node {
                        properties: node.properties.clone(),
                    });
                }
                if let [child] = node.children.as_slice() {
                    end = *child;
                } else {
                    break;
                }
            }
            let children = &self.arena[end.0 as usize].children;
            if visited {
                let children = children
                    .iter()
                    .filter_map(|id| built[id.0 as usize].take())
                    .collect();
                built[start.0 as usize] = Some(GameTree { sequence, children });
            } else {
                pending.push((start, true));
                pending.extend(children.iter().rev().map(|id| (*id, false)));
            }
        }
        // The root always exists, even in an empty game.
        built[self.root.0 as usize].take().unwrap_or(GameTree {
            sequence: vec![Node::default()],
            children: Vec::new(),
        })
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn current(&self) -> NodeId {
        self.current
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn node(&self, id: NodeId) -> Result<&DocumentNode, DocError> {
        self.arena
            .get(id.0 as usize)
            .filter(|node| node.live)
            .ok_or(DocError::MissingNode(id))
    }

    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            root: self.root,
            current: self.current,
            nodes: self
                .arena
                .iter()
                .filter(|node| node.live)
                .cloned()
                .collect(),
        }
    }

    /// Replays on demand: edits, undo and redo cannot leave a stale cache.
    pub fn position_at(&self, id: NodeId) -> Result<GamePosition, DocError> {
        let mut ancestors = Vec::new();
        let mut next = Some(id);
        while let Some(id) = next {
            let node = self.node(id)?;
            ancestors.push((id, node.properties.as_slice()));
            next = node.parent;
        }
        semantics::replay(ancestors.into_iter().rev())
    }

    /// An invalid node and its descendants report the offending node/reason.
    /// Navigation still permits visiting them to inspect comments/properties.
    pub fn node_error(&self, id: NodeId) -> Result<Option<DocError>, DocError> {
        self.node(id)?;
        Ok(self.position_at(id).err())
    }

    /// Recoverable metadata issues across all live branches, even after an
    /// illegal move. Recomputed without mutating the document or accumulating
    /// duplicates; deleting and restoring a subtree updates its warnings too.
    pub fn warnings(&self) -> Vec<DocumentWarning> {
        semantics::warnings(
            self.root,
            self.arena
                .iter()
                .filter(|node| node.live)
                .map(|node| (node.id, node.properties.as_slice())),
        )
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn allocate(
        &mut self,
        parent: Option<NodeId>,
        properties: Vec<sgf::Property>,
    ) -> Result<NodeId, DocError> {
        let id = NodeId(u32::try_from(self.arena.len()).map_err(|_| DocError::NodeLimit)?);
        self.arena.push(DocumentNode {
            id,
            parent,
            children: Vec::new(),
            properties,
            live: true,
        });
        Ok(id)
    }
}
