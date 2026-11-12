//! Undoable analysis removal kept alongside file state without changing node IDs.

use super::Actor;
use crate::{AnalysisSummary, AppError, StoredAnalysis};
use document::{GameDocument, NodeId};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(super) struct Edits {
    pub cleared: BTreeMap<NodeId, u64>,
    pub version: u64,
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    managed: bool,
}

#[derive(Clone)]
enum Entry {
    Document,
    Clear(Change),
}

#[derive(Clone)]
struct Change {
    focus: NodeId,
    fingerprint: Option<String>,
    nodes: Vec<Before>,
    token: u64,
}

#[derive(Clone)]
struct Before {
    node: NodeId,
    cleared: Option<u64>,
    result: Option<StoredAnalysis>,
    summary: Option<AnalysisSummary>,
}

impl Edits {
    pub fn document_edit(&mut self) {
        self.managed = true;
        self.undo.push(Entry::Document);
        self.redo.clear();
    }

    pub fn can_undo(&self, document: &GameDocument) -> bool {
        !self.undo.is_empty() || (!self.managed && document.can_undo())
    }

    pub fn can_redo(&self, document: &GameDocument) -> bool {
        !self.redo.is_empty() || (!self.managed && document.can_redo())
    }

    pub fn active_clears(&self, document: &GameDocument) -> BTreeMap<NodeId, u64> {
        self.cleared
            .iter()
            .filter(|(id, _)| document.node(**id).is_ok())
            .map(|(&id, &token)| (id, token))
            .collect()
    }

    pub fn strip(&self, document: &GameDocument, tree: &mut sgf::GameTree) {
        visit_tree(document, tree, |id, properties| {
            if self.cleared.contains_key(&id) {
                properties.retain(|p| !is_analysis(&p.identifier));
            }
        });
    }

    pub fn effective_tree(&self, document: &GameDocument) -> sgf::GameTree {
        let mut tree = document.to_sgf();
        self.strip(document, &mut tree);
        tree
    }
}

pub(super) fn is_analysis(name: &str) -> bool {
    matches!(name, "LZ" | "LZOP" | "LZ2" | "LZOP2")
}

/// SGF export preserves the document's preorder while compressing single-child
/// chains into sequences. Pair those existing nodes without parsing or renumbering.
pub(super) fn visit_tree(
    document: &GameDocument,
    tree: &mut sgf::GameTree,
    mut visit: impl FnMut(NodeId, &mut Vec<sgf::Property>),
) {
    let mut ids = Vec::new();
    let mut pending = vec![document.root()];
    while let Some(id) = pending.pop() {
        if let Ok(node) = document.node(id) {
            ids.push(id);
            pending.extend(node.children().iter().rev().copied());
        }
    }
    let mut ids = ids.into_iter();
    let mut pending = vec![tree];
    while let Some(tree) = pending.pop() {
        for node in &mut tree.sequence {
            if let Some(id) = ids.next() {
                visit(id, &mut node.properties);
            }
        }
        pending.extend(tree.children.iter_mut().rev());
    }
}

impl Actor {
    pub(super) fn is_analysis_cleared(&self, node: NodeId) -> bool {
        self.file_state.analysis_edits.cleared.contains_key(&node)
    }

    pub(super) async fn clear_analysis(&mut self, all: bool) -> Result<(), AppError> {
        self.next_revision()?;
        let ids = if all {
            self.document
                .variation_tree()?
                .into_iter()
                .map(|node| node.id)
                .collect()
        } else {
            vec![self.document.current()]
        };
        let mut nodes = Vec::new();
        for node in ids {
            let cleared = self.file_state.analysis_edits.cleared.get(&node).copied();
            let has_raw = cleared.is_none()
                && self
                    .document
                    .node(node)?
                    .properties()
                    .iter()
                    .any(|p| is_analysis(&p.identifier));
            let result = self.review.results.get(&node).cloned();
            let summary = self.summaries.get(&node).copied();
            if has_raw || result.is_some() || summary.is_some() {
                nodes.push(Before {
                    node,
                    cleared,
                    result,
                    summary,
                });
            }
        }
        if nodes.is_empty() {
            return Ok(());
        }
        let token = self
            .file_state
            .analysis_edits
            .version
            .checked_add(1)
            .ok_or(AppError::RevisionLimit)?;
        let change = Change {
            focus: self.document.current(),
            fingerprint: self.summary_fingerprint.clone(),
            nodes,
            token,
        };
        self.apply_clear(&change);
        self.file_state.analysis_edits.managed = true;
        self.file_state
            .analysis_edits
            .undo
            .push(Entry::Clear(change));
        self.file_state.analysis_edits.redo.clear();
        self.analysis_edit_changed().await
    }

    fn apply_clear(&mut self, change: &Change) {
        for before in &change.nodes {
            self.file_state
                .analysis_edits
                .cleared
                .insert(before.node, change.token);
            self.review.results.remove(&before.node);
            self.summaries.remove(&before.node);
            self.graph_analysis.valid_nodes.remove(&before.node);
        }
        self.file_state.analysis_edits.version = change.token;
    }

    pub(super) async fn edit_history(&mut self, undo: bool) -> Result<(), AppError> {
        self.next_revision()?;
        let edits = &self.file_state.analysis_edits;
        let entry = if undo {
            edits.undo.last()
        } else {
            edits.redo.last()
        }
        .cloned();
        match entry {
            Some(Entry::Clear(change)) => {
                // Navigation is not a separate undo item; restore the focus at
                // which this analysis edit was made, just like domain edits.
                self.document.apply(document::Command::GoTo(change.focus))?;
                if undo {
                    for before in &change.nodes {
                        match before.cleared {
                            Some(token) => {
                                self.file_state
                                    .analysis_edits
                                    .cleared
                                    .insert(before.node, token);
                            }
                            None => {
                                self.file_state.analysis_edits.cleared.remove(&before.node);
                            }
                        }
                        self.review.results.remove(&before.node);
                        self.summaries.remove(&before.node);
                        if let Some(result) = &before.result
                            && (result.saved.is_some()
                                || change.fingerprint == self.summary_fingerprint)
                        {
                            self.review.results.insert(before.node, result.clone());
                            if let Some(summary) = before.summary {
                                self.summaries.insert(before.node, summary);
                            }
                        }
                    }
                } else {
                    self.apply_clear(&change);
                }
                self.file_state.analysis_edits.version = self.revision + 1;
                self.move_history_entry(undo);
                self.graph_analysis.clear();
                self.analysis_edit_changed().await?;
            }
            Some(Entry::Document) => {
                if undo {
                    self.document.undo()?;
                } else {
                    self.document.redo()?;
                }
                self.move_history_entry(undo);
                self.document_changed().await?;
            }
            None => {
                if edits.managed {
                    return Err(if undo {
                        document::DocError::NothingToUndo
                    } else {
                        document::DocError::NothingToRedo
                    }
                    .into());
                }
                if undo {
                    self.document.undo()?;
                } else {
                    self.document.redo()?;
                }
                self.document_changed().await?;
            }
        }
        Ok(())
    }

    fn move_history_entry(&mut self, undo: bool) {
        let edits = &mut self.file_state.analysis_edits;
        let (from, to) = if undo {
            (&mut edits.undo, &mut edits.redo)
        } else {
            (&mut edits.redo, &mut edits.undo)
        };
        if let Some(entry) = from.pop() {
            to.push(entry);
        }
    }

    async fn analysis_edit_changed(&mut self) -> Result<(), AppError> {
        self.flush_cached_focus().await;
        self.cancel_auto_curve_intent();
        self.cancel_review().await;
        self.file_state.save_warnings.clear();
        self.focus = None;
        self.output.clear_analysis();
        // Drop old cache/pending snapshots while retaining automatic limit
        // state and the user's analysis intent. Persistent storage is untouched.
        if let Some(running) = &self.running
            && let Err(error) = running.coordinator.discard_cached_analysis().await
        {
            self.analysis_failed(error.to_string());
        }
        self.document_changed().await
    }
}

#[cfg(test)]
mod tests;
