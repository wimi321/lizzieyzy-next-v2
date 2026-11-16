//! Comparison state is owned by the application actor, separately from review.
use super::{Actor, PendingTrial, Running, Signal};
use crate::{AnalysisView, AppEvent, ComparisonView, EngineStatus, StoredAnalysis};
use analysis::{AnalysisSnapshot, AnalysisStatus, SnapshotSource};
use document::NodeId;
use std::collections::{BTreeSet, HashMap};

mod document_state;
mod lifecycle;
mod saved;

pub(super) use document_state::DocumentState;

#[cfg(test)]
#[path = "comparison/freshness_tests.rs"]
mod freshness_tests;

pub(super) struct State {
    pub running: Option<Running>,
    pub trial: Option<PendingTrial>,
    pub status: EngineStatus,
    pub fingerprint: Option<String>,
    pub results: HashMap<NodeId, StoredAnalysis>,
    pub replaced_raw: BTreeSet<NodeId>,
    pub latest: Option<AnalysisView>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            running: None,
            trial: None,
            status: EngineStatus::Stopped,
            fingerprint: None,
            results: HashMap::new(),
            replaced_raw: BTreeSet::new(),
            latest: None,
        }
    }
}

impl Actor {
    pub(super) fn double_engine(&self) -> bool {
        self.settings.display.layout_mode == crate::LayoutMode::DoubleEngine
    }

    pub(super) fn comparison_view(&self) -> ComparisonView {
        ComparisonView {
            engine_id: self.settings.comparison_engine_id.clone(),
            status: self.comparison.status.clone(),
            analysis: self.comparison_focus_view(),
        }
    }

    pub(super) fn publish_comparison_changed(&self) {
        self.output
            .send(AppEvent::ComparisonChanged(self.comparison_view()));
    }

    pub(super) fn comparison_failed(&mut self, reason: String) {
        self.comparison.status = EngineStatus::Failed {
            reason,
            stderr_tail: Vec::new(),
        };
        self.publish_comparison_changed();
    }

    pub(super) fn set_comparison_fingerprint(&mut self, profile: Option<String>) {
        let fingerprint = profile.and_then(|profile| self.evaluation_fingerprint(&profile).ok());
        if self.comparison.fingerprint != fingerprint {
            self.comparison.fingerprint = fingerprint;
            self.comparison.results.retain(|_, result| {
                result.saved.is_some() && self.settings.search_params.is_none()
            });
            self.comparison.latest = None;
            self.output.clear_comparison_analysis();
        }
    }

    pub(super) async fn comparison_signal(&mut self, signal: Signal) {
        let current = self.refresh_comparison_identity().await;
        if !current
            && matches!(
                signal,
                Signal::Engine(engine_session::EngineStatus::Ready { .. })
            )
        {
            return;
        }
        match signal {
            Signal::Snapshot(snapshot) if current => self.comparison_snapshot(*snapshot),
            Signal::Engine(status) => {
                self.comparison.status = status.into();
                self.publish_comparison_changed();
            }
            Signal::Analysis(AnalysisStatus::Failed(error)) => {
                if let Some(running) = &self.comparison.running
                    && let engine_session::EngineStatus::Failed {
                        reason,
                        stderr_tail,
                    } = running.engine_status.borrow().clone()
                {
                    self.comparison.status = EngineStatus::Failed {
                        reason,
                        stderr_tail,
                    };
                    self.publish_comparison_changed();
                    return;
                }
                self.comparison_failed(error.to_string());
            }
            _ => {}
        }
    }

    fn comparison_snapshot(&mut self, snapshot: AnalysisSnapshot) {
        let pure = snapshot.source == SnapshotSource::PureNetwork;
        let restricted =
            snapshot.source == SnapshotSource::Restricted || (pure && self.restriction.is_some());
        if !self.double_engine()
            || pure != self.config.analysis.pure_network
            || (restricted && self.restriction.is_none())
            || (!self.pondering() && snapshot.source == SnapshotSource::Live)
        {
            return;
        }
        let Some(focus) = &self.focus else {
            return;
        };
        if (focus.revision, focus.node_id, focus.key)
            != (snapshot.revision, snapshot.node_id.0, snapshot.key)
        {
            return;
        }
        let mut view =
            match AnalysisView::from_snapshot(snapshot.clone(), focus.size, focus.to_play) {
                Ok(view) => view,
                Err(error) => {
                    self.comparison_failed(format!("invalid analysis coordinate: {error}"));
                    return;
                }
            };
        if !pure && !restricted {
            let preserve = self.can_reuse_saved_analysis()
                && self
                    .comparison
                    .results
                    .get(&snapshot.node_id)
                    .is_some_and(|stored| {
                        stored.key == snapshot.key
                            && stored.saved.is_some()
                            && stored.response.root_info.visits
                                >= snapshot.response.root_info.visits
                    });
            if preserve {
                if snapshot.source == SnapshotSource::Extras
                    && let Some(stored) = self.comparison.results.get_mut(&snapshot.node_id)
                {
                    stored.response.policy = snapshot.response.policy.clone();
                    for candidate in &mut stored.response.move_infos {
                        candidate.ownership = snapshot
                            .response
                            .move_infos
                            .iter()
                            .find(|item| item.coordinate == candidate.coordinate)
                            .and_then(|item| item.ownership.clone());
                    }
                    self.comparison.latest = None;
                }
                self.publish_comparison_focus();
                return;
            }
            let replace = self
                .comparison
                .results
                .get(&snapshot.node_id)
                .is_none_or(|stored| {
                    stored.key != snapshot.key
                        || stored.response.root_info.visits <= snapshot.response.root_info.visits
                        || (stored.saved.is_some() && !self.can_reuse_saved_analysis())
                });
            if replace {
                self.comparison.replaced_raw.insert(snapshot.node_id);
                self.comparison.results.insert(
                    snapshot.node_id,
                    StoredAnalysis {
                        node_id: snapshot.node_id,
                        key: snapshot.key,
                        response: snapshot.response.clone(),
                        saved: None,
                    },
                );
            }
            if let Some(best) = self.comparison.results.get(&snapshot.node_id) {
                view.summary = crate::AnalysisSummary {
                    winrate: best.response.root_info.winrate,
                    score_lead: best.response.root_info.score_lead,
                    visits: best.response.root_info.visits,
                    score_lead_available: best
                        .saved
                        .as_ref()
                        .is_none_or(|saved| saved.score_lead.is_some()),
                };
            }
            if snapshot.source == SnapshotSource::Live
                && self.settings.cache_enabled
                && let Some(cache) = &self.cache
                && let Some(running) = &self.comparison.running
                && let Some(fingerprint) = self.comparison_cache_fingerprint(&running.fingerprint)
            {
                cache.put(snapshot.key, &fingerprint, snapshot.response.clone());
            }
        }
        if snapshot.node_id == self.document.current() {
            self.comparison.latest = Some(view.clone());
            self.output.send(AppEvent::ComparisonAnalysisUpdated(view));
        }
    }

    pub(super) async fn rebuild_comparison_coordinator(&mut self) {
        let Some(mut running) = self.comparison.running.take() else {
            return;
        };
        self.set_comparison_fingerprint(Some(running.fingerprint.clone()));
        match self.coordinator(&running.engine, &running.fingerprint) {
            Ok((coordinator, snapshots)) => {
                let previous = std::mem::replace(&mut running.coordinator, coordinator);
                running.analysis_status = running.coordinator.status();
                running.snapshots = snapshots;
                self.comparison.running = Some(running);
                if let Err(error) = previous.shutdown().await {
                    self.comparison_failed(error.to_string());
                }
            }
            Err(error) => {
                self.comparison.running = Some(running);
                self.comparison_failed(error.to_string());
            }
        }
    }

    pub(super) async fn reset_comparison_document(&mut self) {
        self.comparison.results.clear();
        self.comparison.replaced_raw.clear();
        self.comparison.latest = None;
        self.output.clear_comparison_analysis();
        self.rebuild_comparison_coordinator().await;
    }
}
