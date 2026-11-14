use super::Actor;
use crate::{
    AnalysisSource, AnalysisView, AppError, AppEvent, GraphView, StoredAnalysis, WholeGameError,
    WholeGameProgress, WholeGameScope, WholeGameSource, WholeGameStatus, review::ReviewInput,
};
use analysis::{
    AnalysisSnapshot, SnapshotSource,
    review::{ReviewBatch, ReviewPlan, ReviewResult, ReviewRun},
};
use document::{GamePosition, NodeId};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::pending,
};

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct ReviewState {
    pub results: HashMap<NodeId, StoredAnalysis>,
    pub progress: Option<WholeGameProgress>,
    pub(super) line: Vec<(NodeId, GamePosition)>,
    force: bool,
    completed: HashSet<NodeId>,
    batches: VecDeque<ReviewBatch>,
    active: Option<ReviewRun>,
}

pub(super) async fn receive(
    state: &mut ReviewState,
) -> Option<Result<Vec<ReviewResult>, analysis::AnalysisError>> {
    match &mut state.active {
        Some(active) => active.recv().await,
        None => pending().await,
    }
}

impl Actor {
    /// Invalidate the old document/engine namespace, including a finished task.
    /// Keep a cancellation snapshot for event subscribers, never an old Completed
    /// state paired with an empty result table in the new namespace.
    pub(super) fn invalidate_review(&mut self) {
        self.graph_analysis.clear();
        self.review.results.clear();
        self.review.line.clear();
        self.review.completed.clear();
        self.review.batches.clear();
        if self.review.progress.is_some() {
            self.set_review_status(WholeGameStatus::Cancelled);
        }
    }

    pub(super) fn stored_analysis(&self, node_id: u32) -> Option<StoredAnalysis> {
        let id = NodeId(node_id);
        let stored = self.review.results.get(&id)?;
        let position = self.document.position_at(id).ok()?;
        (position.analysis_key() == stored.key).then(|| stored.clone())
    }

    pub(super) fn review_view(&self, node_id: u32) -> Result<Option<AnalysisView>, AppError> {
        let Some(stored) = self.stored_analysis(node_id) else {
            return Ok(None);
        };
        let position = self.document.position_at(stored.node_id)?;
        let mut view = AnalysisView::from_snapshot(
            AnalysisSnapshot {
                revision: self.revision,
                node_id: stored.node_id,
                key: stored.key,
                response: stored.response,
                source: SnapshotSource::Cache,
                visits_per_second: None,
            },
            position.board.size(),
            position.board.to_play(),
        )
        .map_err(analysis::AnalysisError::from)?;
        if let Some(summary) = self.summaries.get(&stored.node_id) {
            view.summary = *summary;
        }
        if let Some(saved) = &stored.saved {
            super::game_files::mark_saved_view(&mut view, saved);
        }
        Ok(Some(view))
    }

    /// Called only after coordinates and current document identity are checked.
    pub(super) fn retain_analysis(&mut self, snapshot: &AnalysisSnapshot) {
        if self.preserves_saved_analysis(snapshot) {
            return;
        }
        // Both live and review callers already checked this result against the
        // current document. Publishing its graph point needs no second replay.
        self.graph_analysis.valid_nodes.insert(snapshot.node_id);
        if self
            .review
            .results
            .get(&snapshot.node_id)
            .is_some_and(|stored| stored.key != snapshot.key)
        {
            self.summaries.remove(&snapshot.node_id);
        }
        // A deeper engine result replaces the saved payload and its metadata.
        if self
            .review
            .results
            .get(&snapshot.node_id)
            .is_some_and(|stored| stored.saved.is_some())
        {
            self.review.results.remove(&snapshot.node_id);
            self.summaries.remove(&snapshot.node_id);
        }
        let candidate = StoredAnalysis {
            node_id: snapshot.node_id,
            key: snapshot.key,
            response: snapshot.response.clone(),
            saved: None,
        };
        self.review
            .results
            .entry(snapshot.node_id)
            .and_modify(|best| {
                if best.key != candidate.key
                    || candidate.response.root_info.visits >= best.response.root_info.visits
                {
                    *best = candidate.clone();
                }
            })
            .or_insert(candidate);
    }

    pub(super) async fn review_command(
        &mut self,
        input: ReviewInput,
    ) -> Result<WholeGameProgress, AppError> {
        self.ensure_no_batch()?;
        match input {
            ReviewInput::Start(options) => {
                self.ensure_no_benchmark()?;
                options.validate()?;
                let line = self.review_positions(options.scope)?;
                self.start_review(line, options.visits, WholeGameSource::Manual, options.force)
                    .await?;
            }
            ReviewInput::Pause => return self.pause_review().await,
            ReviewInput::Resume => return self.resume_review().await,
            ReviewInput::Cancel => {
                if self.review.progress.is_none() {
                    return Err(WholeGameError::NotStarted.into());
                }
                self.cancel_auto_curve_intent();
                self.cancel_review().await;
            }
        }
        self.review
            .progress
            .clone()
            .ok_or_else(|| WholeGameError::NotStarted.into())
    }

    pub(super) fn review_positions(
        &self,
        scope: WholeGameScope,
    ) -> Result<Vec<(NodeId, GamePosition)>, AppError> {
        let ids = match scope {
            WholeGameScope::AllBranches => self
                .document
                .variation_tree()?
                .into_iter()
                .map(|node| node.id)
                .collect(),
            WholeGameScope::Line | WholeGameScope::Range { .. } => {
                let graph = GraphView::from_document(&self.document)?;
                let (first, last) = if let WholeGameScope::Range {
                    first_move,
                    last_move,
                } = scope
                {
                    let available = graph.points.last().map_or(0, |point| point.move_number);
                    let first = first_move.unwrap_or(1);
                    let last = last_move.unwrap_or(available);
                    if first == 0 || first > last || last > available {
                        return Err(WholeGameError::InvalidRange {
                            first_move: first,
                            last_move: last,
                            last_available: available,
                        }
                        .into());
                    }
                    (first, last)
                } else {
                    (0, usize::MAX)
                };
                // A move is evaluated against its direct parent, which can be
                // an empty/setup node after the preceding numbered move. Keep
                // exactly that preceding position, not every node at its number.
                // Starting at move one also includes the root/opening context.
                let start = if first <= 1 {
                    0
                } else {
                    graph
                        .points
                        .partition_point(|point| point.move_number < first)
                        .saturating_sub(1)
                };
                graph
                    .points
                    .into_iter()
                    .skip(start)
                    .take_while(|point| point.move_number <= last)
                    .map(|point| NodeId(point.node_id))
                    .collect::<Vec<_>>()
            }
        };
        ids.into_iter()
            .map(|id| Ok((id, self.document.position_at(id)?)))
            .collect()
    }

    pub(super) async fn pause_review(&mut self) -> Result<WholeGameProgress, AppError> {
        if self.review.progress.is_none() {
            return Err(WholeGameError::NotStarted.into());
        }
        if self.review_running() {
            match self.stop_review_query().await {
                Ok(()) => self.set_review_status(WholeGameStatus::Paused),
                Err(error) => self.set_review_status(WholeGameStatus::Error {
                    reason: error.to_string(),
                }),
            }
        }
        self.review
            .progress
            .clone()
            .ok_or_else(|| WholeGameError::NotStarted.into())
    }

    pub(super) async fn resume_review(&mut self) -> Result<WholeGameProgress, AppError> {
        self.ensure_no_benchmark()?;
        let progress = self
            .review
            .progress
            .as_ref()
            .ok_or(WholeGameError::NotStarted)?;
        if progress.status != WholeGameStatus::Paused {
            return Err(WholeGameError::NotPaused.into());
        }
        if let Some(progress) = &mut self.review.progress {
            progress.status = WholeGameStatus::Running;
        }
        self.continue_review().await;
        self.review
            .progress
            .clone()
            .ok_or_else(|| WholeGameError::NotStarted.into())
    }

    pub(super) async fn start_review(
        &mut self,
        line: Vec<(NodeId, GamePosition)>,
        visits: u64,
        source: WholeGameSource,
        force: bool,
    ) -> Result<(), AppError> {
        let running = self.running.as_ref().ok_or(WholeGameError::NoEngine)?;
        if matches!(
            *running.engine_status.borrow(),
            engine_session::EngineStatus::Failed { .. } | engine_session::EngineStatus::Stopped
        ) {
            return Err(WholeGameError::NoEngine.into());
        }
        // Validate before replacing useful work or cancelling pending auto intent.
        ReviewPlan::new(line.clone(), visits)?;
        if source == WholeGameSource::Manual {
            self.cancel_auto_curve_intent();
        }
        self.cancel_review().await;
        self.review.line = line;
        self.review.force = force;
        self.review.completed.clear();
        if force {
            // This is a task boundary, not a per-result write: invalidate old
            // best-depth caches before fresh results start arriving. Otherwise
            // a lower-depth forced replacement could be undone on navigation.
            if let Some(cache) = &self.cache {
                let keys: HashSet<_> = self
                    .review
                    .line
                    .iter()
                    .map(|(_, position)| position.analysis_key())
                    .collect();
                for key in keys {
                    cache.clear_position(key).await;
                }
            }
            if let Some(running) = &self.running {
                running.coordinator.discard_cached_analysis().await?;
            }
            self.output.clear_analysis();
        } else {
            self.restore_cached_review().await?;
        }
        self.review.progress = Some(WholeGameProgress {
            source,
            completed: 0,
            total: self.review.line.len(),
            visits,
            status: WholeGameStatus::Running,
        });
        self.continue_review().await;
        Ok(())
    }

    /// Do not reintroduce pre-force results into the freshly cleared live LRU
    /// when the user browses a selected node before its new Final arrives.
    pub(super) fn force_review_pending(&self, node: NodeId) -> bool {
        self.review.force
            && self.review.progress.as_ref().is_some_and(|progress| {
                matches!(
                    progress.status,
                    WholeGameStatus::Running | WholeGameStatus::Paused
                )
            })
            && !self.review.completed.contains(&node)
            && self.review.line.iter().any(|(id, _)| *id == node)
    }

    pub(super) fn review_running(&self) -> bool {
        self.review
            .progress
            .as_ref()
            .is_some_and(|progress| progress.status == WholeGameStatus::Running)
    }

    pub(super) async fn stop_review_query(&mut self) -> Result<(), AppError> {
        self.review.batches.clear();
        if let Some(mut active) = self.review.active.take()
            && let Some(running) = &self.running
        {
            active.cancel(&running.engine).await?;
        }
        Ok(())
    }

    pub(super) async fn cancel_review(&mut self) {
        if self
            .review
            .progress
            .as_ref()
            .is_some_and(|p| matches!(p.status, WholeGameStatus::Running | WholeGameStatus::Paused))
        {
            match self.stop_review_query().await {
                Ok(()) => self.set_review_status(WholeGameStatus::Cancelled),
                Err(error) => self.set_review_status(WholeGameStatus::Error {
                    reason: error.to_string(),
                }),
            }
        }
    }

    fn set_review_status(&mut self, status: WholeGameStatus) {
        if let Some(progress) = &mut self.review.progress {
            progress.status = status;
            self.output
                .send(AppEvent::WholeGameProgress(progress.clone()));
        }
    }

    pub(super) async fn continue_review(&mut self) {
        if !self.review_running() {
            return;
        }
        if let Err(error) = self.prepare_review().await {
            let _ = self.stop_review_query().await;
            self.set_review_status(WholeGameStatus::Error {
                reason: error.to_string(),
            });
        }
    }

    async fn prepare_review(&mut self) -> Result<(), AppError> {
        let Some(progress) = self.review.progress.as_ref() else {
            return Ok(());
        };
        let visits = progress.visits;
        let mut remaining = Vec::new();
        for (id, position) in &self.review.line {
            let current = self
                .document
                .position_at(*id)
                .map_err(|_| WholeGameError::ChangedPosition)?;
            if current.analysis_key() != position.analysis_key() {
                return Err(WholeGameError::ChangedPosition.into());
            }
            if !self.review_node_complete(*id, position, visits) {
                remaining.push((*id, position.clone()));
            }
        }
        if let Some(progress) = &mut self.review.progress {
            progress.completed = progress.total - remaining.len();
        }
        if remaining.is_empty() {
            self.stop_review_query().await?;
            self.set_review_status(WholeGameStatus::Completed);
            return Ok(());
        }
        self.review.batches = ReviewPlan::new(remaining, visits)?.into_batches().into();
        self.submit_review_batch().await?;
        self.set_review_status(WholeGameStatus::Running);
        Ok(())
    }

    fn review_node_complete(&self, id: NodeId, position: &GamePosition, visits: u64) -> bool {
        if self.review.force {
            return self.review.completed.contains(&id);
        }
        self.review.results.get(&id).is_some_and(|result| {
            result.key == position.analysis_key()
                && (result.saved.is_none() || self.can_reuse_saved_analysis())
                && result.response.root_info.visits >= visits
        })
    }

    async fn submit_review_batch(&mut self) -> Result<(), AppError> {
        if let Some(mut batch) = self.review.batches.pop_front() {
            let settings = self.live_settings();
            if let Some(rules) = settings.query_rules {
                batch.request.rules = rules;
            }
            if let Some(komi) = settings.query_komi {
                batch.request.komi = Some(komi);
            }
            let interval = settings.report_interval.as_secs_f64();
            batch.request.report_during_search_every = Some(interval);
            batch.request.first_report_during_search_after = Some(interval);
            let running = self.running.as_ref().ok_or(WholeGameError::NoEngine)?;
            self.review.active = Some(batch.submit(&running.engine).await?);
        }
        Ok(())
    }

    pub(super) async fn review_signal(
        &mut self,
        result: Option<Result<Vec<ReviewResult>, analysis::AnalysisError>>,
    ) {
        if !self.review_running() {
            return;
        }
        let outcome = match result {
            Some(Ok(results)) => self.accept_review_results(results),
            Some(Err(error)) => Err(error.into()),
            None => {
                self.review.active = None;
                // Live analysis may have completed nodes in a later batch
                // since the plan was built. Recheck depth before submitting.
                self.prepare_review().await
            }
        };
        if self
            .review
            .progress
            .as_ref()
            .is_some_and(|progress| progress.status == WholeGameStatus::Completed)
        {
            // Live snapshots can satisfy turns that the batch is still running.
            // Always terminate the remaining wire query before dropping it.
            let _ = self.stop_review_query().await;
        }
        if let Err(error) = outcome {
            let _ = self.stop_review_query().await;
            self.set_review_status(WholeGameStatus::Error {
                reason: error.to_string(),
            });
        }
    }

    fn accept_review_results(&mut self, results: Vec<ReviewResult>) -> Result<(), AppError> {
        let current = self.document.current().0;
        let had_saved_focus = self
            .stored_analysis(current)
            .is_some_and(|stored| stored.saved.is_some());
        let mut refreshed_focus = false;
        for result in results {
            let position = self
                .document
                .position_at(result.node_id)
                .map_err(|_| WholeGameError::ChangedPosition)?;
            if position.analysis_key() != result.key {
                return Err(WholeGameError::ChangedPosition.into());
            }
            let snapshot = AnalysisSnapshot {
                revision: self.revision,
                node_id: result.node_id,
                key: result.key,
                response: result.response,
                source: SnapshotSource::Cache,
                visits_per_second: None,
            };
            if !self.review.force && self.preserves_saved_analysis(&snapshot) {
                continue;
            }
            let view = AnalysisView::from_snapshot(
                snapshot.clone(),
                position.board.size(),
                position.board.to_play(),
            )
            .map_err(analysis::AnalysisError::from)?;
            // Forced reviews replace even deeper old file/engine results, but
            // only after the fresh result and its coordinates have validated.
            if self.review.force {
                self.review.results.remove(&result.node_id);
                self.summaries.remove(&result.node_id);
            }
            self.retain_analysis(&snapshot);
            self.review.completed.insert(result.node_id);
            refreshed_focus |= result.node_id.0 == current;
            self.persist_analysis(&snapshot);
            self.summaries
                .entry(result.node_id)
                .and_modify(|best| {
                    if view.summary.visits >= best.visits {
                        *best = view.summary;
                    }
                })
                .or_insert(view.summary);
        }
        // Graph events never change the focused board or document revision.
        self.publish_document()?;
        // When this batch improves the displayed file result, let its full
        // candidates replace Saved too. Other review results do not take focus.
        if (had_saved_focus || (self.review.force && refreshed_focus))
            && let Some(view) = self.review_view(current)?
            && view.source != AnalysisSource::Saved
        {
            self.output.send(AppEvent::AnalysisUpdated(view));
        }
        let completed = self.review.progress.as_ref().map_or(0, |progress| {
            self.review
                .line
                .iter()
                .filter(|(id, position)| self.review_node_complete(*id, position, progress.visits))
                .count()
        });
        if let Some(progress) = &mut self.review.progress {
            progress.completed = completed;
            if progress.completed == progress.total {
                progress.status = WholeGameStatus::Completed;
            }
            self.output
                .send(AppEvent::WholeGameProgress(progress.clone()));
        }
        Ok(())
    }
}
