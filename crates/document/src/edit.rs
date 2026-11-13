use go_core::{Color, Move};
use sgf::Property;

use crate::{Command, DocError, GameDocument, NodeId, Revision};

/// Reversible deltas, rather than cloning the document for each command.
#[derive(Debug, Clone)]
pub(crate) struct Edit {
    pub(crate) before: NodeId,
    pub(crate) after: NodeId,
    pub(crate) change: Change,
}

#[derive(Debug, Clone)]
pub(crate) enum Change {
    Add {
        id: NodeId,
        parent: NodeId,
    },
    Properties {
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
    MainPath(Vec<(NodeId, usize)>),
    Nodes(Vec<crate::move_editing::NodePatch>),
    PropertiesMany(Vec<crate::everyday::PropertyPatch>),
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
            Command::Play(mv) => match self.prepare_play(mv, true)? {
                PreparedPlay::Existing(id) => return Ok(self.navigate(id, next_revision)),
                PreparedPlay::New(edit) => Some(edit),
            },
            Command::PlayColored(mv) => match self.prepare_play(mv, false)? {
                PreparedPlay::Existing(id) => return Ok(self.navigate(id, next_revision)),
                PreparedPlay::New(edit) => Some(edit),
            },
            Command::InsertMove(mv) => Some(self.prepare_insert_move(mv)?),
            Command::SwitchStone(point) => self.prepare_stone_edit(point, None, true)?,
            Command::DeleteStone(point) => self.prepare_stone_edit(point, None, false)?,
            Command::MoveStone { from, to } => self.prepare_stone_edit(from, Some(to), false)?,
            Command::AddVariation(moves) => self.prepare_add_variation(&moves)?,
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
                        if !inserted && !comment.is_empty() {
                            updated.push(comment_property(&comment));
                            inserted = true;
                        }
                    } else {
                        updated.push(property.clone());
                    }
                }
                if !inserted && !comment.is_empty() {
                    updated.push(comment_property(&comment));
                }
                (updated != *properties).then(|| Edit {
                    before,
                    after: before,
                    change: Change::Properties {
                        id: before,
                        before: properties.clone(),
                        after: updated,
                    },
                })
            }
            Command::DeleteBranch(id) => {
                let start = self.branch_start(id)?;
                Some(self.prepare_delete(start)?)
            }
            Command::PromoteToMain(id) => {
                self.sibling_index(id)?;
                let mut swaps = Vec::new();
                let mut node = id;
                while node != self.root {
                    let (parent, index) = self.sibling_index(node)?;
                    if index != 0 {
                        swaps.push((parent, index));
                    }
                    node = parent;
                }
                (!swaps.is_empty()).then_some(Edit {
                    before,
                    after: before,
                    change: Change::MainPath(swaps),
                })
            }
            Command::SetGameInfo(info) => {
                let properties = &self.node(self.root)?.properties;
                let updated = self.updated_game_info(info)?;
                (updated != *properties).then(|| Edit {
                    before,
                    after: before,
                    change: Change::Properties {
                        id: self.root,
                        before: properties.clone(),
                        after: updated,
                    },
                })
            }
            Command::SetSetupStone { point, color } => {
                self.property_edit(self.root, self.updated_setup_stone(point, color)?)?
            }
            Command::ClearSetup => self.property_edit(self.root, self.cleared_setup()?)?,
            Command::SetFirstPlayer(color) => {
                self.property_edit(self.root, self.updated_first_player(color)?)?
            }
            Command::SetMarkup { point, kind } => {
                self.property_edit(before, self.updated_markup(point, kind)?)?
            }
            Command::ClearMarkup => self.property_edit(before, self.cleared_markup()?)?,
            Command::ToggleToPlay => self.prepare_toggle_to_play()?,
            Command::Transform(transform) => self.prepare_transform(transform)?,
            Command::DeleteNode(id) => Some(self.prepare_delete(id)?),
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
            self.content_revision = next_revision;
        }
        Ok(self.revision)
    }

    pub(crate) fn property_edit(
        &self,
        id: NodeId,
        updated: Vec<Property>,
    ) -> Result<Option<Edit>, DocError> {
        let properties = self.node(id)?.properties();
        Ok((updated != properties).then(|| Edit {
            before: self.current,
            after: self.current,
            change: Change::Properties {
                id,
                before: properties.to_vec(),
                after: updated,
            },
        }))
    }

    fn prepare_delete(&self, id: NodeId) -> Result<Edit, DocError> {
        let before = self.current;
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
        Ok(Edit {
            before,
            after,
            change: Change::Delete { id, parent, index },
        })
    }

    fn branch_start(&self, id: NodeId) -> Result<NodeId, DocError> {
        self.sibling_index(id)?;
        let mut node = id;
        while let Some(parent) = self.node(node)?.parent {
            if self.node(parent)?.children.len() > 1 {
                return Ok(node);
            }
            node = parent;
        }
        Ok(id)
    }

    pub fn undo(&mut self) -> Result<Revision, DocError> {
        let revision = self.next_revision()?;
        let edit = self.undo.pop().ok_or(DocError::NothingToUndo)?;
        self.execute(&edit, false);
        self.redo.push(edit);
        self.revision = revision;
        self.content_revision = revision;
        Ok(revision)
    }

    pub fn redo(&mut self) -> Result<Revision, DocError> {
        let revision = self.next_revision()?;
        let edit = self.redo.pop().ok_or(DocError::NothingToRedo)?;
        self.execute(&edit, true);
        self.undo.push(edit);
        self.revision = revision;
        self.content_revision = revision;
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

    fn prepare_play(&mut self, mv: Move, check_turn: bool) -> Result<PreparedPlay, DocError> {
        let mut position = self.position_at(self.current)?;
        if check_turn && mv.color() != position.board.to_play() {
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
            Change::Nodes(patches) => self.execute_node_patches(patches, forward),
            Change::PropertiesMany(patches) => {
                for patch in patches {
                    self.arena[patch.id.0 as usize].properties =
                        if forward { &patch.after } else { &patch.before }.clone();
                }
            }
            Change::Add { id, parent } => {
                if forward {
                    self.arena[parent.0 as usize].children.push(*id);
                } else {
                    self.arena[parent.0 as usize].children.pop();
                }
                self.set_live(*id, forward);
            }
            Change::Properties { id, before, after } => {
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
            Change::MainPath(swaps) => {
                for (parent, index) in swaps {
                    self.arena[parent.0 as usize].children.swap(0, *index);
                }
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
