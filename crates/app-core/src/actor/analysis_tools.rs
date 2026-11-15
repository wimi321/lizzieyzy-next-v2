use super::Actor;
use crate::{
    AnalysisRestriction, AnalysisToolsError, AppError, LadderContinuation, LadderOutcome,
    PointView, RestrictionDepth, RestrictionKind, SearchParams, analysis_tools::Request,
};

impl Actor {
    pub(super) async fn analysis_tools_command(&mut self, request: Request) {
        match request {
            Request::Set(kind, points, reply) => {
                let _ = reply.send(self.change_restriction(kind, points, false).await);
            }
            Request::Add(kind, points, reply) => {
                let _ = reply.send(self.change_restriction(kind, points, true).await);
            }
            Request::Remove(points, reply) => {
                let _ = reply.send(self.remove_restriction(points).await);
            }
            Request::Clear(reply) => {
                let result = self.ensure_research_available();
                let result = match result {
                    Ok(()) => self.install_restriction(None).await,
                    Err(e) => Err(e),
                };
                let _ = reply.send(result);
            }
            Request::Depth(kind, depth, reply) => {
                let _ = reply.send(self.restriction_depth(kind, depth).await);
            }
            Request::Search(params, reply) => {
                let _ = reply.send(self.change_search_params(params).await);
            }
            Request::Ladder(reply) => {
                let _ = reply.send(self.continue_ladder().await);
            }
        }
    }

    pub(super) fn ensure_research_available(&self) -> Result<(), AppError> {
        if self
            .game
            .as_ref()
            .is_some_and(|game| !matches!(game.view.phase, crate::GamePhase::Finished))
            || self.match_active()
            || self.coach.is_starting()
            || self.coach.session_active()
        {
            return Err(AnalysisToolsError::GameActive.into());
        }
        if self.updates.install_pending {
            return Err(crate::updates::UpdateError::Busy.into());
        }
        self.ensure_no_batch()?;
        self.ensure_no_benchmark()
    }

    fn checked_points(&self, mut points: Vec<PointView>) -> Result<Vec<PointView>, AppError> {
        let size = self
            .document
            .position_at(self.document.current())?
            .board
            .size();
        if points
            .iter()
            .any(|p| p.x >= size.width || p.y >= size.height)
        {
            return Err(AnalysisToolsError::InvalidPoint.into());
        }
        points.sort_by_key(|p| (p.y, p.x));
        points.dedup();
        Ok(points)
    }

    fn depth_for(&self, kind: RestrictionKind) -> RestrictionDepth {
        match kind {
            RestrictionKind::Allow => self.settings.restriction_depths.allow,
            RestrictionKind::Avoid => self.settings.restriction_depths.avoid,
        }
    }

    pub(super) fn engine_restriction(&self) -> Option<analysis::AnalysisRestriction> {
        self.restriction
            .as_ref()
            .map(|r| analysis::AnalysisRestriction {
                kind: match r.kind {
                    RestrictionKind::Allow => analysis::RestrictionKind::Allow,
                    RestrictionKind::Avoid => analysis::RestrictionKind::Avoid,
                },
                points: r
                    .points
                    .iter()
                    .map(|p| go_core::Point { x: p.x, y: p.y })
                    .collect(),
                depth: r.depth,
            })
    }

    async fn change_restriction(
        &mut self,
        kind: RestrictionKind,
        mut points: Vec<PointView>,
        add: bool,
    ) -> Result<Option<AnalysisRestriction>, AppError> {
        self.ensure_research_available()?;
        if add
            && let Some(existing) = &self.restriction
            && existing.kind == kind
        {
            points.extend(existing.points.iter().copied());
        }
        let points = self.checked_points(points)?;
        let next = (!points.is_empty()).then(|| AnalysisRestriction {
            kind,
            points,
            depth: self.depth_for(kind).value(),
        });
        self.install_restriction(next).await?;
        Ok(self.restriction.clone())
    }

    async fn remove_restriction(
        &mut self,
        points: Vec<PointView>,
    ) -> Result<Option<AnalysisRestriction>, AppError> {
        self.ensure_research_available()?;
        let points = self.checked_points(points)?;
        let next = self.restriction.clone().and_then(|mut r| {
            r.points.retain(|p| !points.contains(p));
            (!r.points.is_empty()).then_some(r)
        });
        self.install_restriction(next).await?;
        Ok(self.restriction.clone())
    }

    async fn install_restriction(
        &mut self,
        next: Option<AnalysisRestriction>,
    ) -> Result<(), AppError> {
        if self.restriction == next {
            return Ok(());
        }
        self.flush_cached_focus().await;
        self.restriction = next;
        self.restricted_analysis = None;
        self.output.clear_analysis();
        if let Some(running) = &self.running {
            running
                .coordinator
                .set_restriction(self.engine_restriction())
                .await?;
        }
        self.publish_document()?;
        self.update_focus().await;
        Ok(())
    }

    async fn restriction_depth(
        &mut self,
        kind: RestrictionKind,
        depth: RestrictionDepth,
    ) -> Result<(), AppError> {
        self.ensure_research_available()?;
        depth.validate()?;
        let mut settings = self.settings.clone();
        match kind {
            RestrictionKind::Allow => settings.restriction_depths.allow = depth,
            RestrictionKind::Avoid => settings.restriction_depths.avoid = depth,
        }
        settings.save(&self.directory).await?;
        self.settings = settings;
        if let Some(mut restriction) = self.restriction.clone()
            && restriction.kind == kind
        {
            restriction.depth = depth.value();
            self.install_restriction(Some(restriction)).await?;
        }
        Ok(())
    }

    async fn change_search_params(
        &mut self,
        params: Option<SearchParams>,
    ) -> Result<Option<SearchParams>, AppError> {
        self.ensure_research_available()?;
        if let Some(p) = &params {
            p.validate()?;
        }
        let params = params.filter(|p| !p.is_empty());
        if params == self.settings.search_params {
            return Ok(params);
        }
        let mut settings = self.settings.clone();
        settings.search_params = params.clone();
        settings.save(&self.directory).await?;
        self.flush_cached_analysis().await;
        self.cancel_review().await;
        self.settings = settings;
        self.restricted_analysis = None;
        // No-engine documents still have Saved results even though their
        // summary fingerprint is None. Always invalidate this identity change.
        self.invalidate_review();
        self.summaries.clear();
        self.set_summary_fingerprint(None);
        self.output.clear_analysis();
        self.focus = None;
        if let Some(mut running) = self.running.take() {
            let (coordinator, snapshots) =
                match self.coordinator(&running.engine, &running.fingerprint) {
                    Ok(replacement) => replacement,
                    Err(error) => {
                        self.running = Some(running);
                        return Err(error);
                    }
                };
            running.analysis_status = coordinator.status();
            let previous = std::mem::replace(&mut running.coordinator, coordinator);
            running.snapshots = snapshots;
            self.set_summary_fingerprint(Some(running.fingerprint.clone()));
            self.running = Some(running);
            if let Err(error) = previous.shutdown().await {
                self.analysis_failed(error.to_string());
            }
        }
        if self.settings.search_params.is_none() {
            self.import_saved_analysis()?;
        }
        self.publish_document()?;
        self.update_focus().await;
        Ok(params)
    }

    async fn continue_ladder(&mut self) -> Result<LadderContinuation, AppError> {
        self.ensure_research_available()?;
        let position = self.document.position_at(self.document.current())?;
        let reading = go_core::ladder::read_ladder(&position.board, position.board.to_play());
        let outcome = match reading.outcome {
            go_core::ladder::LadderOutcome::Captured => LadderOutcome::Captured,
            go_core::ladder::LadderOutcome::Escaped => LadderOutcome::Escaped,
            go_core::ladder::LadderOutcome::NotLadder => {
                return Err(AnalysisToolsError::NotLadder.into());
            }
        };
        let moves = reading.moves.len();
        self.apply(super::Input::Edit(document::Command::ContinueSequence(
            reading.moves,
        )))
        .await?;
        Ok(LadderContinuation { outcome, moves })
    }
}

#[cfg(test)]
mod tests;
