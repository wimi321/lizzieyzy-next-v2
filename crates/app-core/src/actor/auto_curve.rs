//! One load-scoped automatic review intent, owned by the application actor.

use super::Actor;
use crate::{AppError, StoredAnalysis, WholeGameSource, WholeGameStatus};
use document::{AnalysisKey, GameDocument, GamePosition, NodeId};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct State {
    loaded: Option<blake3::Hash>,
    cancelled: Option<blake3::Hash>,
    pub pending: bool,
}

impl Actor {
    pub(super) async fn set_auto_quick_analyze_on_load(
        &mut self,
        enabled: bool,
    ) -> Result<(), AppError> {
        if self.settings.auto_quick_analyze_on_load != enabled {
            let mut settings = self.settings.clone();
            settings.auto_quick_analyze_on_load = enabled;
            settings.save(&self.directory).await?;
            self.settings = settings;
        }
        if !enabled {
            self.auto_curve.pending = false;
            if self
                .review
                .progress
                .as_ref()
                .is_some_and(|p| p.source == WholeGameSource::AutomaticQuick)
            {
                self.cancel_review().await;
            }
        }
        Ok(())
    }

    pub(super) async fn open_auto_curve_document(
        &mut self,
        document: GameDocument,
        file_name: Option<String>,
    ) -> Result<(), AppError> {
        // Canonical first-game SGF makes byte encoding, collection neighbours and
        // the load route irrelevant, without guessing identity from an empty root.
        let identity = blake3::hash(
            sgf::serialize(&sgf::Collection {
                games: vec![document.to_sgf()],
                encoding: "UTF-8".into(),
                warnings: Vec::new(),
            })
            .as_bytes(),
        );
        let manual = self.manual_review_active();
        // Reuse only the immediately loaded same game. Results already belong to
        // summary_fingerprint; rebind by replay key, never by the old arena's IDs.
        let cached = if self.auto_curve.loaded == Some(identity) {
            best_by_key(self.review.results.values())
        } else {
            HashMap::new()
        };
        self.next_revision()?;
        self.document = document;
        self.file_name = file_name;
        self.tsumego = None;
        self.document_replaced_with_cache(cached).await?;
        if self.auto_curve.loaded != Some(identity) {
            self.auto_curve.cancelled = None;
        }
        self.auto_curve.loaded = Some(identity);
        self.auto_curve.pending = !manual
            && self.settings.auto_quick_analyze_on_load
            && self.auto_curve.cancelled != Some(identity);
        self.maybe_start_auto_curve().await;
        Ok(())
    }

    pub(super) fn restore_loaded_analysis(
        &mut self,
        cached: HashMap<AnalysisKey, StoredAnalysis>,
    ) -> Result<(), AppError> {
        if cached.is_empty() {
            return Ok(());
        }
        for node in self.document.variation_tree()? {
            let id = node.id;
            let Ok(position) = self.document.position_at(id) else {
                continue;
            };
            if let Some(stored) = cached.get(&position.analysis_key()) {
                let mut stored = stored.clone();
                stored.node_id = id;
                self.review.results.insert(id, stored);
                if let Some(view) = self.review_view(id.0)? {
                    self.summaries.insert(id, view.summary);
                }
            }
        }
        Ok(())
    }

    fn manual_review_active(&self) -> bool {
        self.review.progress.as_ref().is_some_and(|p| {
            p.source == WholeGameSource::Manual
                && matches!(p.status, WholeGameStatus::Running | WholeGameStatus::Paused)
        })
    }

    /// Only explicit user cancellation/manual takeover records suppression.
    /// Internal cancellation (load, engine switch, shutdown) must not do so.
    pub(super) fn cancel_auto_curve_intent(&mut self) {
        if self.auto_curve.pending
            || self
                .review
                .progress
                .as_ref()
                .is_some_and(|p| p.source == WholeGameSource::AutomaticQuick)
        {
            self.auto_curve.cancelled = self.auto_curve.loaded;
        }
        self.auto_curve.pending = false;
    }

    pub(super) async fn maybe_start_auto_curve(&mut self) {
        if !self.auto_curve.pending {
            return;
        }
        if !self.settings.auto_quick_analyze_on_load || self.manual_review_active() {
            self.auto_curve.pending = false;
            return;
        }
        if self.benchmark_active()
            || self.running.as_ref().is_none_or(|running| {
                !matches!(
                    *running.engine_status.borrow(),
                    engine_session::EngineStatus::Starting
                        | engine_session::EngineStatus::Ready { .. }
                )
            })
        {
            return;
        }
        // Consume exactly once: navigation, completion and later Ready events
        // cannot start another automatic task for this load.
        self.auto_curve.pending = false;
        let Ok(line) = self.auto_curve_line() else {
            return;
        };
        // Match Java: empty/setup-only games do not request a quick review.
        if !line.iter().any(|(_, position)| !position.moves.is_empty())
            || line.iter().all(|(id, position)| {
                self.review.results.get(id).is_some_and(|stored| {
                    stored.key == position.analysis_key() && stored.response.root_info.visits > 0
                })
            })
        {
            return;
        }
        // Automatic work never changes pondering and uses the normal low-priority
        // review plan. Invalid imported lines stay inspectable without auto work.
        let _ = self
            .start_review(line, 1, WholeGameSource::AutomaticQuick)
            .await;
    }

    fn auto_curve_line(&self) -> Result<Vec<(NodeId, GamePosition)>, AppError> {
        let mut line = Vec::new();
        let mut id = self.document.root();
        loop {
            line.push((id, self.document.position_at(id)?));
            match self.document.node(id)?.children().first() {
                Some(child) => id = *child,
                None => return Ok(line),
            }
        }
    }
}

fn best_by_key<'a>(
    results: impl Iterator<Item = &'a StoredAnalysis>,
) -> HashMap<AnalysisKey, StoredAnalysis> {
    let mut cached: HashMap<AnalysisKey, StoredAnalysis> = HashMap::new();
    for stored in results {
        cached
            .entry(stored.key)
            .and_modify(|best| {
                if stored.response.root_info.visits >= best.response.root_info.visits {
                    *best = stored.clone();
                }
            })
            .or_insert_with(|| stored.clone());
    }
    cached
}

#[cfg(test)]
mod tests;
