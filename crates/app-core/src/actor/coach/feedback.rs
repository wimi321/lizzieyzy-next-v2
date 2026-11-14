use super::*;
use crate::{CoachDecision, CoachHint, CoachVerdict, GameError};

impl Actor {
    pub(in crate::actor) async fn coach_after_move(
        &mut self,
        parent: NodeId,
        node: NodeId,
        mv: Move,
        human: bool,
    ) -> Result<(), AppError> {
        if !self.coach.session_active() {
            return Ok(());
        }
        if !human {
            if let Some(session) = &mut self.coach.session {
                session.before = None;
                session.rating = None;
                Self::cancel_coach_query(session.review.take()).await;
            }
            return Ok(());
        }
        if self.coach_session()?.view.setup.mode != CoachMode::LiveFeedback {
            return Ok(());
        }
        let position = self.document.position_at(parent)?;
        let move_number = self.game_view().ok_or(CoachError::NotActive)?.move_number;
        if let Some(session) = &mut self.coach.session {
            session.rating = Some(PendingRating {
                before: parent,
                position: position.clone(),
                mv,
                node,
                move_number,
                deadline: Instant::now() + Duration::from_secs(4),
            });
        }
        let cached = self
            .coach_session()?
            .before
            .as_ref()
            .filter(|(id, _)| *id == parent)
            .map(|(_, r)| r.clone());
        if let Some(before) = cached {
            self.rate_coach_or_query_after(before).await?;
        } else if self.coach_session()?.review.is_none() {
            let rank = self.coach_session()?.view.setup.rank;
            let query = self
                .submit_coach_query(parent, &position, Some(rank), 400, 1.5, None)
                .await?;
            if let Some(session) = &mut self.coach.session {
                session.review = Some(query);
            }
        }
        Ok(())
    }
    pub(super) async fn coach_review_event(
        &mut self,
        event: Option<QueryEvent>,
    ) -> Result<(), AppError> {
        match event {
            Some(QueryEvent::Partial(response)) => {
                if let Some(query) = self.coach.session.as_mut().and_then(|s| s.review.as_mut()) {
                    query.latest = Some(response);
                }
            }
            Some(QueryEvent::Final(response)) => {
                if let Some(query) = self.coach.session.as_mut().and_then(|s| s.review.take()) {
                    if self.document.position_at(query.node)?.analysis_key() != query.key {
                        return Err(GameError::PositionChanged.into());
                    }
                    self.finish_coach_review(query.node, response).await?;
                }
            }
            Some(QueryEvent::Restarted) => {}
            _ => {
                if let Some(session) = &mut self.coach.session {
                    session.review = None;
                    session.rating = None;
                }
            }
        }
        if self.coach.session_active() {
            self.maybe_coach_move().await?;
        }
        Ok(())
    }
    pub(super) async fn finish_coach_review(
        &mut self,
        node: NodeId,
        response: AnalysisResponse,
    ) -> Result<(), AppError> {
        let pending_node = self.coach_session()?.rating.as_ref().map(|r| r.node);
        if pending_node == Some(node) {
            self.emit_coach_rating(Some(&response));
        } else {
            if let Some(session) = &mut self.coach.session {
                session.before = Some((node, response.clone()));
            }
            if self
                .coach_session()?
                .rating
                .as_ref()
                .is_some_and(|r| r.before == node)
            {
                self.rate_coach_or_query_after(response).await?;
            }
        }
        Ok(())
    }
    async fn rate_coach_or_query_after(
        &mut self,
        before: AnalysisResponse,
    ) -> Result<(), AppError> {
        let session = self.coach_session()?;
        let Some(rating) = &session.rating else {
            return Ok(());
        };
        if reviewed_lead(&before, rating.mv, rating.position.board.size()).is_some() {
            self.emit_coach_rating(None);
        } else {
            let node = rating.node;
            let rank = session.view.setup.rank;
            let position = self.document.position_at(node)?;
            let query = self
                .submit_coach_query(node, &position, Some(rank), 200, 0.8, None)
                .await?;
            if let Some(session) = &mut self.coach.session {
                Self::cancel_coach_query(session.review.take()).await;
                session.review = Some(query);
            }
        }
        Ok(())
    }
    fn emit_coach_rating(&mut self, after: Option<&AnalysisResponse>) {
        let Some(session) = &mut self.coach.session else {
            return;
        };
        let Some(pending) = session.rating.take() else {
            return;
        };
        let Some((before_node, before)) = &session.before else {
            return;
        };
        if *before_node != pending.before {
            return;
        }
        let Some(rating) = rate_move(
            before,
            after,
            &pending.position,
            pending.mv,
            pending.node.0,
            pending.move_number,
        ) else {
            return;
        };
        session.view.awaiting_decision = rating.verdict == CoachVerdict::Blunder;
        session.view.last_rating = Some(rating.clone());
        session.ratings.push(rating.clone());
        self.output.send(AppEvent::CoachMoveRated(rating));
        self.publish_coach();
    }
    pub(super) fn coach_hint(&mut self) -> Result<CoachHint, AppError> {
        let session = self.coach_session()?;
        if session.view.setup.mode != CoachMode::LiveFeedback {
            return Err(CoachError::HintUnavailable.into());
        }
        let game = self.game_view().ok_or(CoachError::NotActive)?;
        if game.phase != GamePhase::Playing
            || game.to_play != session.view.human_color
            || self.document.current().0 != game.latest_node
        {
            return Err(CoachError::NotHumanTurn.into());
        }
        let node = NodeId(game.latest_node);
        let position = self.document.position_at(node)?;
        let Some((before_node, before)) = &session.before else {
            return Ok(CoachHint::Reading);
        };
        if *before_node != node {
            return Ok(CoachHint::Reading);
        }
        let best = best_hint(before, &position)?;
        let session = self.coach.session.as_mut().ok_or(CoachError::NotActive)?;
        let count = session.hints.entry(position.analysis_key()).or_default();
        let first = *count == 0;
        if first {
            session.view.hints = session.view.hints.saturating_add(1);
        }
        *count = count.saturating_add(1);
        let mv: Move = best.mv.into();
        let hint = if first && let Move::Play(_, point) = mv {
            let size = position.board.size();
            CoachHint::Region {
                row: (u16::from(point.y) * 3 / u16::from(size.height)) as u8,
                column: (u16::from(point.x) * 3 / u16::from(size.width)) as u8,
            }
        } else {
            CoachHint::Exact(best)
        };
        self.publish_coach();
        Ok(hint)
    }
    pub(super) async fn coach_decision(
        &mut self,
        decision: CoachDecision,
    ) -> Result<CoachHint, AppError> {
        let session = self.coach_session()?;
        if !session.view.awaiting_decision {
            return Err(CoachError::NoDecisionPending.into());
        }
        match decision {
            CoachDecision::Retry => {
                self.take_back().await?;
                Ok(CoachHint::Reading)
            }
            CoachDecision::ShowBest => session
                .view
                .last_rating
                .as_ref()
                .map(|r| CoachHint::Exact(r.best.clone()))
                .ok_or_else(|| CoachError::NoDecisionPending.into()),
            CoachDecision::Continue => {
                if let Some(session) = &mut self.coach.session {
                    session.view.awaiting_decision = false;
                }
                self.publish_coach();
                self.maybe_coach_move().await?;
                Ok(CoachHint::Reading)
            }
        }
    }
}
