use super::Actor;
use crate::{
    AnalysisView, AppError, AppEvent, GraphView, StoredAnalysis, WholeGameError, WholeGameProgress,
    WholeGameStatus, review::ReviewInput,
};
use analysis::{
    AnalysisSnapshot, SnapshotSource,
    review::{ReviewBatch, ReviewPlan, ReviewResult, ReviewRun},
};
use document::{GamePosition, NodeId};
use std::{
    collections::{HashMap, VecDeque},
    future::pending,
};

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct ReviewState {
    pub results: HashMap<NodeId, StoredAnalysis>,
    pub progress: Option<WholeGameProgress>,
    line: Vec<(NodeId, GamePosition)>,
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
        self.review.results.clear();
        self.review.line.clear();
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
            },
            position.board.size(),
            position.board.to_play(),
        )
        .map_err(analysis::AnalysisError::from)?;
        if let Some(summary) = self.summaries.get(&stored.node_id) {
            view.summary = *summary;
        }
        Ok(Some(view))
    }

    /// Called only after coordinates and current document identity are checked.
    pub(super) fn retain_analysis(&mut self, snapshot: &AnalysisSnapshot) {
        let candidate = StoredAnalysis {
            node_id: snapshot.node_id,
            key: snapshot.key,
            response: snapshot.response.clone(),
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
        match input {
            ReviewInput::Start(options) => {
                options.validate()?;
                let running = self.running.as_ref().ok_or(WholeGameError::NoEngine)?;
                if matches!(
                    *running.engine_status.borrow(),
                    engine_session::EngineStatus::Failed { .. }
                        | engine_session::EngineStatus::Stopped
                ) {
                    return Err(WholeGameError::NoEngine.into());
                }
                let graph = GraphView::from_document(&self.document)?;
                let line = graph
                    .points
                    .into_iter()
                    .map(|point| {
                        let id = NodeId(point.node_id);
                        Ok((id, self.document.position_at(id)?))
                    })
                    .collect::<Result<Vec<_>, AppError>>()?;
                // Validate the entire captured line before replacing a useful task.
                ReviewPlan::new(line.clone(), options.visits)?;
                self.cancel_review().await;
                self.review.line = line;
                self.review.progress = Some(WholeGameProgress {
                    completed: 0,
                    total: self.review.line.len(),
                    visits: options.visits,
                    status: WholeGameStatus::Running,
                });
                self.continue_review().await;
            }
            ReviewInput::Pause => {
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
            }
            ReviewInput::Resume => {
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
            }
            ReviewInput::Cancel => {
                if self.review.progress.is_none() {
                    return Err(WholeGameError::NotStarted.into());
                }
                self.cancel_review().await;
            }
        }
        self.review
            .progress
            .clone()
            .ok_or_else(|| WholeGameError::NotStarted.into())
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
            if !self.review.results.get(id).is_some_and(|result| {
                result.key == position.analysis_key() && result.response.root_info.visits >= visits
            }) {
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

    async fn submit_review_batch(&mut self) -> Result<(), AppError> {
        if let Some(batch) = self.review.batches.pop_front() {
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
            };
            let view = AnalysisView::from_snapshot(
                snapshot.clone(),
                position.board.size(),
                position.board.to_play(),
            )
            .map_err(analysis::AnalysisError::from)?;
            self.retain_analysis(&snapshot);
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
        if let Some(progress) = &mut self.review.progress {
            progress.completed = self
                .review
                .line
                .iter()
                .filter(|(id, position)| {
                    self.review.results.get(id).is_some_and(|result| {
                        result.key == position.analysis_key()
                            && result.response.root_info.visits >= progress.visits
                    })
                })
                .count();
            if progress.completed == progress.total {
                progress.status = WholeGameStatus::Completed;
            }
            self.output
                .send(AppEvent::WholeGameProgress(progress.clone()));
        }
        Ok(())
    }
}
