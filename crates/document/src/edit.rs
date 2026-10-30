use go_core::{Color, Move};
use sgf::Property;

use crate::{Command, DocError, GameDocument, NodeId, Revision};

/// Reversible deltas, rather than cloning the document for each command.
#[derive(Debug, Clone)]
pub(crate) struct Edit {
    before: NodeId,
    after: NodeId,
    change: Change,
}

#[derive(Debug, Clone)]
enum Change {
    Add {
        id: NodeId,
        parent: NodeId,
    },
    Comment {
        id: NodeId,
        before: Vec<Property>,
        after: Vec<Property>,
    },
    Delete {
        id: NodeId,
        parent: NodeId,
        index: usize,
    },
    Promote {
        parent: NodeId,
        index: usize,
    },
}

enum PreparedPlay {
    Existing(NodeId),
    New(Edit),
}

impl GameDocument {
    /// Successful changes (including navigation) advance the revision. No-ops
    /// leave revision/history untouched; failures are atomic. Only content
    /// changes enter undo history and clear redo history.
    pub fn apply(&mut self, command: Command) -> Result<Revision, DocError> {
        let next_revision = self.next_revision()?;
        let before = self.current;
        let edit = match command {
            Command::Play(mv) => match self.prepare_play(mv)? {
                PreparedPlay::Existing(id) => return Ok(self.navigate(id, next_revision)),
                PreparedPlay::New(edit) => Some(edit),
            },
            Command::GoTo(id) => {
                self.node(id)?;
                return Ok(self.navigate(id, next_revision));
            }
            Command::Step(steps) => {
                let mut id = before;
                for _ in 0..steps.unsigned_abs() {
                    let node = self.node(id)?;
                    let next = if steps < 0 {
                        node.parent
                    } else {
                        node.children.first().copied()
                    };
                    let Some(next) = next else { break };
                    id = next;
                }
                return Ok(self.navigate(id, next_revision));
            }
            Command::SetComment(comment) => {
                let properties = &self.node(before)?.properties;
                let mut updated = Vec::with_capacity(properties.len() + 1);
                let mut inserted = false;
                for property in properties {
                    if property.identifier == "C" {
                        if !inserted {
                            updated.push(comment_property(&comment));
                            inserted = true;
                        }
                    } else {
                        updated.push(property.clone());
                    }
                }
                if !inserted {
                    updated.push(comment_property(&comment));
                }
                (updated != *properties).then(|| Edit {
                    before,
                    after: before,
                    change: Change::Comment {
                        id: before,
                        before: properties.clone(),
                        after: updated,
                    },
                })
            }
            Command::DeleteNode(id) => {
                let (parent, index) = self.sibling_index(id)?;
                let mut after = before;
                let mut ancestor = Some(before);
                while let Some(current) = ancestor {
                    if current == id {
                        after = parent;
                        break;
                    }
                    ancestor = self.node(current)?.parent;
                }
                Some(Edit {
                    before,
                    after,
                    change: Change::Delete { id, parent, index },
                })
            }
            Command::PromoteVariation(id) => {
                let (parent, index) = self.sibling_index(id)?;
                (index != 0).then_some(Edit {
                    before,
                    after: before,
                    change: Change::Promote { parent, index },
                })
            }
        };
        if let Some(edit) = edit {
            self.execute(&edit, true);
            self.undo.push(edit);
            self.redo.clear();
            self.revision = next_revision;
        }
        Ok(self.revision)
    }

    pub fn undo(&mut self) -> Result<Revision, DocError> {
        let revision = self.next_revision()?;
        let edit = self.undo.pop().ok_or(DocError::NothingToUndo)?;
        self.execute(&edit, false);
        self.redo.push(edit);
        self.revision = revision;
        Ok(revision)
    }

    pub fn redo(&mut self) -> Result<Revision, DocError> {
        let revision = self.next_revision()?;
        let edit = self.redo.pop().ok_or(DocError::NothingToRedo)?;
        self.execute(&edit, true);
        self.undo.push(edit);
        self.revision = revision;
        Ok(revision)
    }

    fn next_revision(&self) -> Result<Revision, DocError> {
        self.revision.checked_add(1).ok_or(DocError::RevisionLimit)
    }

    fn navigate(&mut self, after: NodeId, next_revision: Revision) -> Revision {
        if self.current != after {
            self.current = after;
            self.revision = next_revision;
        }
        self.revision
    }

    fn prepare_play(&mut self, mv: Move) -> Result<PreparedPlay, DocError> {
        let mut position = self.position_at(self.current)?;
        if mv.color() != position.board.to_play() {
            return Err(DocError::WrongPlayer {
                node: self.current,
                expected: position.board.to_play(),
                actual: mv.color(),
            });
        }
        // Even when an SGF child matches, a user command must obey full rules.
        position
            .board
            .play(mv)
            .map_err(|reason| DocError::IllegalMove {
                node: self.current,
                reason,
            })?;
        for child in self.node(self.current)?.children() {
            if self.move_at(*child, position.board.size()) == Some(mv) {
                self.position_at(*child)?;
                return Ok(PreparedPlay::Existing(*child));
            }
        }
        let value = mv
            .to_sgf(position.board.size())
            .map_err(|_| DocError::InvalidProperty {
                node: self.current,
                property: "move".to_owned(),
                value: format!("{mv:?}"),
            })?;
        let id = self.allocate(
            Some(self.current),
            vec![Property {
                identifier: match mv.color() {
                    Color::Black => "B",
                    Color::White => "W",
                }
                .to_owned(),
                values: vec![value],
            }],
        )?;
        Ok(PreparedPlay::New(Edit {
            before: self.current,
            after: id,
            change: Change::Add {
                id,
                parent: self.current,
            },
        }))
    }

    fn move_at(&self, id: NodeId, size: go_core::BoardSize) -> Option<Move> {
        let mut moves = self.arena[id.0 as usize]
            .properties
            .iter()
            .filter(|property| matches!(property.identifier.as_str(), "B" | "W"));
        let property = moves.next()?;
        if moves.next().is_some() || property.values.len() != 1 {
            return None;
        }
        let color = if property.identifier == "B" {
            Color::Black
        } else {
            Color::White
        };
        Move::from_sgf(color, &property.values[0], size).ok()
    }

    fn sibling_index(&self, id: NodeId) -> Result<(NodeId, usize), DocError> {
        let parent = self.node(id)?.parent.ok_or(DocError::RootOperation)?;
        let index = self
            .node(parent)?
            .children
            .iter()
            .position(|child| *child == id)
            .ok_or(DocError::MissingNode(id))?;
        Ok((parent, index))
    }

    fn set_live(&mut self, id: NodeId, live: bool) {
        let mut pending = vec![id];
        while let Some(id) = pending.pop() {
            let node = &mut self.arena[id.0 as usize];
            node.live = live;
            pending.extend(node.children.iter().copied());
        }
    }

    fn execute(&mut self, edit: &Edit, forward: bool) {
        match &edit.change {
            Change::Add { id, parent } => {
                if forward {
                    self.arena[parent.0 as usize].children.push(*id);
                } else {
                    self.arena[parent.0 as usize].children.pop();
                }
                self.set_live(*id, forward);
            }
            Change::Comment { id, before, after } => {
                self.arena[id.0 as usize].properties = if forward { after } else { before }.clone();
            }
            Change::Delete { id, parent, index } => {
                if forward {
                    self.arena[parent.0 as usize].children.remove(*index);
                } else {
                    self.arena[parent.0 as usize].children.insert(*index, *id);
                }
                self.set_live(*id, !forward);
            }
            Change::Promote { parent, index } => {
                let siblings = &mut self.arena[parent.0 as usize].children;
                if forward {
                    let id = siblings.remove(*index);
                    siblings.insert(0, id);
                } else {
                    let id = siblings.remove(0);
                    siblings.insert(*index, id);
                }
            }
        }
        self.current = if forward { edit.after } else { edit.before };
    }
}

fn comment_property(comment: &str) -> Property {
    Property {
        identifier: "C".to_owned(),
        values: vec![comment.to_owned()],
    }
}

#[cfg(test)]
mod tests;
