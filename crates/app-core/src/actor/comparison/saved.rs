use super::*;
use crate::AppError;
use document::{GamePosition, SavedAnalysis};
use katago_protocol::AnalysisResponse;
use std::collections::BTreeMap;

impl Actor {
    pub(in crate::actor) fn valid_all_saved_nodes(&self) -> super::super::game_files::SavedNodes {
        super::super::game_files::SavedNodes {
            primary: self.valid_saved_nodes(),
            comparison: self
                .comparison
                .results
                .keys()
                .copied()
                .filter(|node| {
                    self.comparison_stored(*node)
                        .is_some_and(|stored| stored.saved.is_some())
                })
                .collect(),
        }
    }

    pub(in crate::actor) fn comparison_stored(&self, node: NodeId) -> Option<&StoredAnalysis> {
        let stored = self.comparison.results.get(&node)?;
        (self.document.position_at(node).ok()?.analysis_key() == stored.key).then_some(stored)
    }

    pub(in crate::actor) fn comparison_focus_view(&self) -> Option<AnalysisView> {
        if !self.double_engine() || self.scoring.is_some() || self.coach_hides_analysis() {
            return None;
        }
        let node = self.document.current();
        if let Some(view) = &self.comparison.latest
            && (view.revision, view.node_id) == (self.revision, node.0)
        {
            return Some(view.clone());
        }
        if self.config.analysis.pure_network || self.restriction.is_some() {
            return None;
        }
        let stored = self.comparison_stored(node)?;
        let position = self.document.position_at(node).ok()?;
        let mut view = AnalysisView::from_snapshot(
            AnalysisSnapshot {
                revision: self.revision,
                node_id: node,
                key: stored.key,
                response: stored.response.clone(),
                source: SnapshotSource::Cache,
                visits_per_second: None,
            },
            position.board.size(),
            position.board.to_play(),
        )
        .ok()?;
        if let Some(saved) = &stored.saved {
            super::super::game_files::mark_saved_view(&mut view, saved);
        }
        Some(view)
    }

    pub(in crate::actor) fn publish_comparison_focus(&self) {
        if let Some(view) = self.comparison_focus_view() {
            self.output.send(AppEvent::ComparisonAnalysisUpdated(view));
        }
    }

    pub(in crate::actor) fn import_comparison_saved(
        &mut self,
        selected: Option<&[NodeId]>,
    ) -> Result<(), AppError> {
        self.comparison.latest = None;
        self.output.clear_comparison_analysis();
        if self.settings.search_params.is_some() {
            return Ok(());
        }
        let Self {
            document,
            comparison,
            file_state,
            ..
        } = self;
        document.visit_saved_comparison_analysis::<AppError>(|node, position, saved| {
            if selected.is_some_and(|nodes| !nodes.contains(&node))
                || file_state.analysis_edits.cleared.contains_key(&node)
            {
                return Ok(());
            }
            let key = position.analysis_key();
            if comparison.results.get(&node).is_some_and(|stored| {
                stored.key == key
                    && stored.saved.is_none()
                    && stored.response.root_info.visits > saved.visits
            }) {
                return Ok(());
            }
            let response = super::super::game_files::response_from_saved(&saved, position);
            comparison.results.insert(
                node,
                StoredAnalysis {
                    node_id: node,
                    key,
                    response,
                    saved: Some(saved),
                },
            );
            comparison.replaced_raw.remove(&node);
            Ok(())
        })
    }

    pub(in crate::actor) async fn comparison_cached_focus(
        &self,
        node: NodeId,
        position: &GamePosition,
    ) -> Option<AnalysisResponse> {
        if !self.settings.cache_enabled || self.restriction.is_some() {
            return None;
        }
        let key = position.analysis_key();
        let retained = self
            .comparison
            .results
            .get(&node)
            .filter(|stored| stored.key == key && stored.saved.is_none())
            .map(|stored| stored.response.clone());
        if self.is_analysis_cleared(node) {
            return retained;
        }
        let fingerprint = self
            .comparison
            .running
            .as_ref()
            .and_then(|running| self.comparison_cache_fingerprint(&running.fingerprint));
        let persisted = match (&self.cache, fingerprint) {
            (Some(cache), Some(fingerprint)) => cache.get(key, &fingerprint).await,
            _ => None,
        };
        let persisted = persisted.filter(|response| {
            AnalysisView::from_snapshot(
                AnalysisSnapshot {
                    revision: self.revision,
                    node_id: node,
                    key,
                    response: response.clone(),
                    source: SnapshotSource::Cache,
                    visits_per_second: None,
                },
                position.board.size(),
                position.board.to_play(),
            )
            .is_ok()
        });
        match (retained, persisted) {
            (Some(a), Some(b)) => Some(if a.root_info.visits >= b.root_info.visits {
                a
            } else {
                b
            }),
            (a, b) => a.or(b),
        }
    }

    pub(in crate::actor) fn comparison_export(&self) -> BTreeMap<NodeId, SavedAnalysis> {
        self.comparison
            .results
            .keys()
            .filter_map(|node| {
                let stored = self.comparison_stored(*node)?;
                if stored.saved.is_none() && !self.can_reuse_saved_analysis() {
                    return None;
                }
                Some((
                    *node,
                    stored.saved.clone().unwrap_or_else(|| {
                        super::super::game_files::saved_from_response(&stored.response)
                    }),
                ))
            })
            .collect()
    }

    /// The retained comparison table already chooses the deepest result under
    /// the correct position and evaluation identity. Never compare a fresh
    /// result against raw SGF that may belong to an edited position or an old
    /// identity, and never let a dropped/replaced saved slot reappear on save.
    pub(in crate::actor) fn export_comparison_analysis(
        &self,
        tree: &mut sgf::GameTree,
        warnings: &mut Vec<document::DocumentWarning>,
    ) -> Result<(), AppError> {
        let saved = self.comparison_export();
        let mut replacements = BTreeMap::new();
        let mut changed = BTreeMap::new();
        for node in self.document.variation_tree()? {
            if !self.comparison.replaced_raw.contains(&node.id)
                && !self.is_analysis_cleared(node.id)
            {
                // No new result was accepted for this slot. In particular,
                // search parameters may have intentionally skipped import;
                // absence from the retained table does not authorize deleting
                // the user's secondary raw properties.
                continue;
            }
            if self
                .comparison_stored(node.id)
                .is_some_and(|stored| stored.saved.is_some())
                && !self.is_analysis_cleared(node.id)
            {
                continue;
            }
            let Ok(position) = self.document.position_at(node.id) else {
                continue;
            };
            changed.insert(node.id, position.board.size());
            if let Some(analysis) = saved.get(&node.id) {
                let (payload, clamped) = document::lz::format_with_warnings(
                    analysis,
                    position.board.size(),
                    position.board.to_play(),
                )?;
                replacements.insert(node.id, payload);
                if clamped {
                    warnings.push(document::DocumentWarning {
                        node: node.id,
                        property: "LZ2".into(),
                        kind: document::WarningKind::OutOfRange {
                            fallback: i32::MAX.to_string(),
                        },
                    });
                }
            }
        }
        super::super::clear_analysis::visit_tree(&self.document, tree, |node, properties| {
            if let Some(size) = changed.get(&node) {
                for property in properties.iter_mut() {
                    if matches!(property.identifier.as_str(), "LZ2" | "LZOP2") {
                        property.values.retain(|value| {
                            document::lz::parse(value, *size, go_core::Color::Black).is_err()
                        });
                    }
                }
                properties.retain(|p| {
                    !matches!(p.identifier.as_str(), "LZ2" | "LZOP2") || !p.values.is_empty()
                });
                if let Some(payload) = replacements.remove(&node) {
                    properties.push(sgf::Property {
                        identifier: "LZ2".into(),
                        values: vec![payload],
                    });
                }
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests;
