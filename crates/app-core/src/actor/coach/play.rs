use super::*;
use crate::{AiPlayer, GameEndReason, GameError, Player};
use serde_json::json;

impl Actor {
    pub(in crate::actor) async fn begin_coach_turn(&mut self) -> Result<bool, AppError> {
        if !self.coach.session_active() {
            return Ok(false);
        }
        let view = self.game_view().ok_or(CoachError::NotActive)?;
        let node = NodeId(view.latest_node);
        let position = self.document.position_at(node)?;
        let player = if position.board.to_play() == Color::Black {
            view.black
        } else {
            view.white
        };
        match player {
            Player::Human => {
                if self.coach_session()?.view.setup.mode == CoachMode::LiveFeedback {
                    self.clear_game_live_query().await?;
                    let query = self
                        .submit_coach_query(
                            node,
                            &position,
                            Some(self.coach_session()?.view.setup.rank),
                            400,
                            1.5,
                            None,
                        )
                        .await?;
                    if let Some(session) = &mut self.coach.session {
                        session.before = None;
                        session.review = Some(query);
                    }
                    return Ok(true);
                }
                Ok(false)
            }
            Player::Ai(AiPlayer::HumanSl(play)) => {
                self.clear_game_live_query().await?;
                let started = Instant::now();
                let mut query = self
                    .submit_coach_query(node, &position, Some(play.rank), 1, 10.0, None)
                    .await?;
                query.deadline = started + Duration::from_secs(10);
                if let Some(session) = &mut self.coach.session {
                    session.ai = Some(AiTurn {
                        position,
                        node,
                        play,
                        query: Some(query),
                        stage: Stage::Policy,
                        started,
                        deadline: started + Duration::from_secs(10),
                        policy: None,
                        pool: Vec::new(),
                        round: 0,
                        last_visits: 1,
                        query_started: started,
                        pass_checked: false,
                        selected: None,
                        ready_at: None,
                    });
                }
                Ok(true)
            }
            _ => {
                Err(CoachError::InvalidSetup("coaching requires a HumanSL opponent".into()).into())
            }
        }
    }
    pub(super) async fn submit_coach_query(
        &self,
        node: NodeId,
        position: &GamePosition,
        rank: Option<crate::CoachRank>,
        visits: u64,
        seconds: f64,
        pool: Option<&[PolicyCandidate]>,
    ) -> Result<Query, AppError> {
        let engine = self.game_engine()?;
        let mut live = self.live_settings();
        live.query_rules = None;
        live.query_komi = None;
        live.include_policy = true;
        let mut request = analysis::play_request(
            position,
            &live,
            Some(visits),
            Some(seconds.max(0.001)),
            false,
        )?;
        if let Some(rank) = rank {
            let settings = request
                .override_settings
                .get_or_insert_with(Default::default);
            settings.insert("humanSLProfile".into(), json!(rank.profile()));
            settings.insert("ignorePreRootHistory".into(), json!(false));
            settings.insert("rootNumSymmetriesToSample".into(), json!(1));
            settings.insert(
                "humanSLRootExploreProbWeightless".into(),
                json!(if rank.elite() { 0.8 } else { 0.5 }),
            );
        }
        if let Some(pool) = pool {
            request.extensions.insert("allowMoves".into(), json!([{"player": if position.board.to_play() == Color::Black {"B"} else {"W"}, "moves": pool.iter().map(|c| &c.coordinate).collect::<Vec<_>>(), "untilDepth": 1}]));
        }
        let subscription = engine.submit(request).await?;
        Ok(Query {
            engine,
            subscription,
            node,
            key: position.analysis_key(),
            deadline: Instant::now() + Duration::from_secs_f64(seconds.max(0.001)),
            latest: None,
        })
    }
    pub(super) async fn coach_ai_event(
        &mut self,
        event: Option<QueryEvent>,
    ) -> Result<(), AppError> {
        match event {
            Some(QueryEvent::Partial(response)) => {
                if let Some(query) = self
                    .coach
                    .session
                    .as_mut()
                    .and_then(|s| s.ai.as_mut())
                    .and_then(|ai| ai.query.as_mut())
                {
                    query.latest = Some(response);
                }
                Ok(())
            }
            Some(QueryEvent::Final(response)) => self.finish_coach_ai_query(response).await,
            Some(QueryEvent::Restarted) => Ok(()),
            Some(QueryEvent::Failed(error)) => Err(GameError::Engine(error.to_string()).into()),
            _ => Err(GameError::Engine("HumanSL query ended without a result".into()).into()),
        }
    }
    pub(super) async fn coach_ai_timeout(&mut self) -> Result<(), AppError> {
        let query = self
            .coach
            .session
            .as_mut()
            .and_then(|s| s.ai.as_mut())
            .and_then(|ai| ai.query.take());
        let response = query.as_ref().and_then(|q| q.latest.clone());
        Self::cancel_coach_query(query).await;
        match response {
            Some(response) => self.finish_coach_ai_query(response).await,
            None => {
                // A verification search may time out after an earlier valid result.
                let has_selection = self
                    .coach
                    .session
                    .as_ref()
                    .and_then(|s| s.ai.as_ref())
                    .is_some_and(|ai| ai.selected.is_some());
                if has_selection {
                    self.schedule_coach_move().await
                } else {
                    Err(GameError::Engine("HumanSL move timed out".into()).into())
                }
            }
        }
    }
    async fn finish_coach_ai_query(&mut self, response: AnalysisResponse) -> Result<(), AppError> {
        self.validate_game_position()?;
        let mut ai = self
            .coach
            .session
            .as_mut()
            .and_then(|s| s.ai.take())
            .ok_or(CoachError::NotActive)?;
        ai.query = None;
        if self.game_view().is_none_or(|g| g.latest_node != ai.node.0)
            || self.document.position_at(ai.node)?.analysis_key() != ai.position.analysis_key()
        {
            return Err(GameError::PositionChanged.into());
        }
        let mut search_visits = None;
        match ai.stage {
            Stage::Policy => {
                ai.policy = Some(response.clone());
                if ai.play.rank.raw_policy() {
                    ai.selected =
                        match sample_raw_policy(&response, &ai.position, self.game_random_mut()?) {
                            Ok(mv) => Some(mv),
                            Err(_) if pass_check_needed(&ai.position) => None,
                            Err(error) => return Err(error.into()),
                        };
                } else {
                    ai.pool = candidate_pool(&response, &ai.position)?;
                    search_visits = Some(64);
                }
            }
            Stage::Search => {
                ai.selected = Some(select_searched_move(
                    &ai.pool,
                    &response,
                    &ai.position,
                    ai.play.rank,
                    self.game_random_mut()?,
                )?);
                ai.round += 1;
                if ai.round <= 4
                    && needs_deeper(
                        &ai.pool,
                        &response,
                        ai.position.board.to_play(),
                        ai.play.rank,
                    )
                {
                    let next = adaptive_visits(
                        ai.last_visits,
                        Instant::now().saturating_duration_since(ai.query_started),
                        ai.deadline.saturating_duration_since(Instant::now()),
                    );
                    if next > ai.last_visits && next <= 4096 {
                        search_visits = Some(next);
                    }
                }
            }
            Stage::Pass => {
                ai.pass_checked = true;
                let share = ai.policy.as_ref().map_or(0.0, human_pass_share);
                if let Some(mv) = pass_override(&response, &ai.position, share) {
                    ai.selected = Some(mv);
                } else if !ai.play.rank.raw_policy() && ai.round == 0 {
                    search_visits = Some(64);
                }
                if let Some(session) = &mut self.coach.session
                    && should_resign(
                        ai.play.rank,
                        &response,
                        &ai.position,
                        &mut session.low_winrates,
                    )
                {
                    let result = if ai.position.board.to_play() == Color::Black {
                        "W+R"
                    } else {
                        "B+R"
                    };
                    return self
                        .finish_game(Some(result.into()), GameEndReason::Resignation)
                        .await;
                }
            }
        }
        let remaining = ai
            .deadline
            .saturating_duration_since(Instant::now())
            .as_secs_f64();
        if !ai.pass_checked && pass_check_needed(&ai.position) && remaining > 0.01 {
            ai.stage = Stage::Pass;
            ai.query_started = Instant::now();
            ai.last_visits = 40;
            ai.query = Some(
                self.submit_coach_query(ai.node, &ai.position, None, 40, remaining, None)
                    .await?,
            );
        } else if let Some(visits) = search_visits.filter(|_| remaining > 0.01) {
            ai.stage = Stage::Search;
            ai.query_started = Instant::now();
            ai.last_visits = visits;
            ai.query = Some(
                self.submit_coach_query(
                    ai.node,
                    &ai.position,
                    Some(ai.play.rank),
                    visits,
                    remaining,
                    Some(&ai.pool),
                )
                .await?,
            );
        }
        if let Some(query) = &mut ai.query {
            query.deadline = query.deadline.min(ai.deadline);
        }
        if let Some(session) = &mut self.coach.session {
            session.ai = Some(ai);
        }
        self.schedule_coach_move().await
    }
    async fn schedule_coach_move(&mut self) -> Result<(), AppError> {
        let Some(ai) = self.coach.session.as_ref().and_then(|s| s.ai.as_ref()) else {
            return Ok(());
        };
        if ai.query.is_some() {
            return Ok(());
        }
        if ai.selected.is_none() {
            return Err(CoachError::NoLegalMove.into());
        }
        let (pace, top, moves, area, started) = (
            ai.play.pace,
            ai.policy.as_ref().map_or(0.0, human_top_share),
            ai.position.moves.len(),
            ai.position.board.stones().len(),
            ai.started,
        );
        let delay = think_delay(pace, top, moves, area, self.game_random_mut()?);
        if let Some(ai) = self.coach.session.as_mut().and_then(|s| s.ai.as_mut()) {
            ai.ready_at = Some(started + delay);
        }
        self.publish_game();
        self.maybe_coach_move().await
    }
    pub(super) async fn maybe_coach_move(&mut self) -> Result<(), AppError> {
        if !self.game_playing() {
            return Ok(());
        }
        let session = self.coach_session()?;
        if session.rating.is_some() || session.view.awaiting_decision {
            return Ok(());
        }
        let ready = session.ai.as_ref().is_some_and(|ai| {
            ai.ready_at.is_some_and(|at| Instant::now() >= at) && ai.query.is_none()
        });
        if !ready {
            return Ok(());
        }
        let ai = self
            .coach
            .session
            .as_mut()
            .and_then(|s| s.ai.take())
            .ok_or(CoachError::NotActive)?;
        let mv = ai.selected.ok_or(CoachError::NoLegalMove)?;
        // Document checks the actual ko history one last time before committing.
        Box::pin(self.append_game_move(mv, false)).await
    }
}
