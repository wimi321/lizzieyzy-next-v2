//! The file analysis adapter feeds the same retained results as engine output.

use super::Actor;
use crate::{AnalysisSource, AnalysisView, AppError, AppEvent, StoredAnalysis};
use document::{GamePosition, SavedAnalysis, SavedCandidate};
use katago_protocol::{AnalysisResponse, MoveInfo, Player, RootInfo};
use std::{collections::BTreeMap, path::Path};

pub(super) fn is_gib(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("gib"))
}

pub(super) fn is_game_file(path: &Path) -> bool {
    is_gib(path)
        || path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("sgf"))
}

impl Actor {
    pub(super) fn import_saved_analysis(&mut self) -> Result<(), AppError> {
        let Self {
            document,
            review,
            summaries,
            graph_analysis,
            revision,
            ..
        } = self;
        document.visit_saved_analysis::<AppError>(|id, position, saved| {
            if review
                .results
                .get(&id)
                .is_some_and(|stored| stored.saved.is_none())
            {
                return Ok(());
            }
            let key = position.analysis_key();
            let response = response_from_saved(&saved, position);
            let snapshot = analysis::AnalysisSnapshot {
                revision: *revision,
                node_id: id,
                key,
                response: response.clone(),
                source: analysis::SnapshotSource::Cache,
                visits_per_second: None,
            };
            let mut view = AnalysisView::from_snapshot(
                snapshot,
                position.board.size(),
                position.board.to_play(),
            )
            .map_err(analysis::AnalysisError::from)?;
            mark_saved_view(&mut view, &saved);
            summaries.insert(id, view.summary);
            graph_analysis.valid_nodes.insert(id);
            review.results.insert(
                id,
                StoredAnalysis {
                    node_id: id,
                    key,
                    response,
                    saved: Some(saved),
                },
            );
            Ok(())
        })?;
        // Cached entries restored just before import and these new entries
        // already passed key/position validation. Do not replay them for the
        // first graph publication of the same content revision.
        graph_analysis.validated_content(document.content_revision());
        Ok(())
    }

    pub(super) fn publish_saved_focus(&self) {
        let node = self.document.current().0;
        if self
            .stored_analysis(node)
            .is_some_and(|stored| stored.saved.is_some())
        {
            match self.review_view(node) {
                Ok(Some(view)) => self.output.send(AppEvent::AnalysisUpdated(view)),
                Ok(None) => {}
                Err(error) => self.analysis_failed(error.to_string()),
            }
        }
    }

    pub(super) fn export_document_with_analysis(
        &self,
    ) -> Result<(sgf::GameTree, Vec<document::DocumentWarning>), AppError> {
        let mut saved = BTreeMap::new();
        for node in self.document.variation_tree()? {
            if let Some(stored) = self.stored_analysis(node.id.0) {
                saved.insert(
                    node.id,
                    stored
                        .saved
                        .unwrap_or_else(|| saved_from_response(&stored.response)),
                );
            }
        }
        // Cleared nodes must lose secondary and malformed slots as well as
        // the primary data. Fresh results still serialize through the shared
        // domain writer rather than reviving any original property.
        let cleared_saved: BTreeMap<_, _> = saved
            .keys()
            .copied()
            .filter(|node| self.is_analysis_cleared(*node))
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|node| saved.remove(&node).map(|value| (node, value)))
            .collect();
        let (mut tree, mut warnings) = self.document.to_sgf_with_analysis_and_warnings(&saved)?;
        let mut replacements = BTreeMap::new();
        for (node, analysis) in cleared_saved {
            let position = self.document.position_at(node)?;
            let (payload, clamped) = document::lz::format_with_warnings(
                &analysis,
                position.board.size(),
                position.board.to_play(),
            )?;
            replacements.insert(node, payload);
            if clamped {
                warnings.push(document::DocumentWarning {
                    node,
                    property: "LZ".into(),
                    kind: document::WarningKind::OutOfRange {
                        fallback: i32::MAX.to_string(),
                    },
                });
            }
        }
        super::clear_analysis::visit_tree(&self.document, &mut tree, |node, properties| {
            if self.is_analysis_cleared(node) {
                properties.retain(|p| !super::clear_analysis::is_analysis(&p.identifier));
                if let Some(payload) = replacements.remove(&node) {
                    properties.push(sgf::Property {
                        identifier: "LZ".into(),
                        values: vec![payload],
                    });
                }
            }
        });
        Ok((tree, warnings))
    }
}

pub(super) fn mark_saved_view(view: &mut AnalysisView, saved: &SavedAnalysis) {
    view.source = AnalysisSource::Saved;
    view.summary.score_lead_available = saved.score_lead.is_some();
    for (candidate, original) in view.candidates.iter_mut().zip(&saved.candidates) {
        candidate.score_lead_available = original.score_lead.is_some();
    }
}

fn response_from_saved(saved: &SavedAnalysis, position: &GamePosition) -> AnalysisResponse {
    let to_play = if position.board.to_play() == go_core::Color::Black {
        Player::Black
    } else {
        Player::White
    };
    AnalysisResponse {
        id: "saved-lz".into(),
        turn_number: position.analysis_moves.len().try_into().unwrap_or(u32::MAX),
        is_during_search: false,
        root_info: RootInfo {
            visits: saved.visits,
            winrate: saved.winrate,
            score_lead: saved.score_lead.unwrap_or(0.0),
            utility: 0.0,
            current_player: to_play,
            score_selfplay: None,
            score_stdev: saved.score_stdev,
            weight: None,
            this_hash: None,
            sym_hash: None,
            raw_winrate: None,
            raw_lead: None,
            raw_score_selfplay: None,
            raw_score_selfplay_stdev: None,
            raw_no_result_prob: None,
            raw_st_score_error: None,
            raw_st_wr_error: None,
            raw_var_time_left: None,
        },
        move_infos: saved
            .candidates
            .iter()
            .map(|candidate| MoveInfo {
                coordinate: candidate.coordinate.clone(),
                visits: candidate.visits,
                winrate: candidate.winrate,
                score_lead: candidate.score_lead.unwrap_or(0.0),
                score_mean: candidate.score_lead,
                score_stdev: candidate.score_stdev,
                prior: candidate.prior,
                order: candidate.order,
                pv: candidate.pv.clone(),
                edge_visits: candidate.edge_visits,
                edge_weight: None,
                weight: None,
                score_selfplay: None,
                utility: None,
                lcb: None,
                utility_lcb: None,
                play_selection_value: None,
                is_symmetry_of: None,
            })
            .collect(),
        ownership: saved.ownership.clone().filter(|values| {
            let size = position.board.size();
            values.len() == usize::from(size.width) * usize::from(size.height)
        }),
        policy: None,
    }
}

fn saved_from_response(response: &AnalysisResponse) -> SavedAnalysis {
    SavedAnalysis {
        engine_name: "KataGo".into(),
        winrate: response.root_info.winrate,
        visits: response.root_info.visits,
        score_lead: Some(response.root_info.score_lead),
        score_stdev: response.root_info.score_stdev,
        ownership: response.ownership.clone(),
        candidates: response
            .move_infos
            .iter()
            .map(|candidate| SavedCandidate {
                coordinate: candidate.coordinate.clone(),
                visits: candidate.visits,
                winrate: candidate.winrate,
                score_lead: Some(candidate.score_lead),
                score_stdev: candidate.score_stdev,
                prior: candidate.prior,
                order: candidate.order,
                edge_visits: candidate.edge_visits,
                pv: candidate.pv.clone(),
                pv_visits: Vec::new(),
            })
            .collect(),
        ..SavedAnalysis::default()
    }
}

#[cfg(test)]
mod tests;
