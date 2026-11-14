use super::*;
use crate::{AnalysisView, KataGoPlay};
use engine_session::{EngineHandle, QueryEvent, QuerySubscription};
use katago_protocol::AnalysisResponse;
use std::{future::pending, time::Duration};

pub(super) struct Query {
    subscription: QuerySubscription,
    engine: EngineHandle,
    node: NodeId,
    key: AnalysisKey,
    play: Option<KataGoPlay>,
    deadline: Instant,
    latest: Option<AnalysisResponse>,
}
pub(in crate::actor) enum Event {
    Query(Box<Option<QueryEvent>>),
    Tick,
    Gtp(Box<super::gtp::Outcome>),
    GtpFailed(GameError),
}

pub(in crate::actor) async fn receive(session: &mut Option<Session>) -> Event {
    let Some(game) = session
        .as_mut()
        .filter(|game| game.view.phase == GamePhase::Playing)
    else {
        return pending().await;
    };
    let clock_deadline = game
        .clocks
        .iter()
        .flatten()
        .filter_map(ClockState::deadline)
        .min();
    let deadline = clock_deadline.map(|deadline| deadline.min(game.tick));
    let deadline = match (deadline, game.query.as_ref().map(|q| q.deadline)) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    };
    let states = game
        .gtp_players
        .values()
        .filter(|state| {
            [&game.setup.black, &game.setup.white].iter().any(|p| {
                matches!(p,
            Player::Ai(crate::AiPlayer::Gtp { engine_id, .. }) if engine_id == &state.id)
            })
        })
        .map(|state| (state.id.clone(), state.handle.status(), state.initialized))
        .collect();
    tokio::select! {
        error = super::gtp::failure(states) => Event::GtpFailed(error),
        answer = async {match &mut game.gtp_query {Some(query)=>query.future.as_mut().await,None=>pending().await}} => Event::Gtp(Box::new(answer)),
        event = async { match &mut game.query {Some(query)=>query.subscription.recv().await,None=>pending().await} } => Event::Query(Box::new(event)),
        () = async {match deadline {Some(deadline)=>tokio::time::sleep_until(deadline).await,None=>pending().await}} => Event::Tick,
    }
}
impl Actor {
    pub(in crate::actor) async fn game_play_now(&mut self) -> Result<(), AppError> {
        let game = self.require_game()?;
        if !self.game_playing() {
            return Err(GameError::NotActive.into());
        }
        if matches!(
            game.player(game.color()),
            Player::Ai(crate::AiPlayer::Gtp { .. })
        ) {
            return Err(GameError::GtpPlayNowUnsupported.into());
        }
        let query = game.query.as_ref().ok_or(GameError::NoAnalysis)?;
        let play = query.play.clone().ok_or(GameError::NoAnalysis)?;
        let response = query.latest.clone().ok_or(GameError::NoAnalysis)?;
        let resigns = self.ai_resigns(&play, &response);
        let mv = if resigns {
            None
        } else {
            Some(self.select_game_move(&play, &response)?)
        };
        self.cancel_game_query().await?;
        if let Some(mv) = mv {
            self.append_game_move(mv, false).await
        } else {
            let color = self.require_game()?.color();
            self.finish_game(
                Some(results::win(opponent(color), "R")),
                GameEndReason::Resignation,
            )
            .await
        }
    }
    pub(in crate::actor) fn game_engine(&self) -> Result<EngineHandle, GameError> {
        if let Some(engine) = self.match_engine_for_turn() {
            return Ok(engine);
        }
        let running = self.running.as_ref().ok_or(GameError::NoEngine)?;
        if matches!(
            *running.engine_status.borrow(),
            engine_session::EngineStatus::Failed { .. } | engine_session::EngineStatus::Stopped
        ) {
            return Err(GameError::NoEngine);
        }
        Ok(running.engine.clone())
    }
    pub(in crate::actor) async fn clear_game_live_query(&mut self) -> Result<(), AppError> {
        self.flush_cached_focus().await;
        if let Some(running) = &self.running {
            running.coordinator.clear_focus().await?;
        }
        self.focus = None;
        self.output.clear_analysis();
        Ok(())
    }
    pub(in crate::actor) async fn cancel_game_query(&mut self) -> Result<(), AppError> {
        self.cancel_coach_queries().await;
        self.cancel_gtp_query().await?;
        if let Some(query) = self.game.as_mut().and_then(|game| game.query.take()) {
            query
                .engine
                .terminate(query.subscription.id().to_owned())
                .await?;
        }
        Ok(())
    }
    pub(super) async fn begin_game_turn(&mut self) -> Result<(), AppError> {
        if self.match_move_limit_reached().await? {
            return Ok(());
        }
        let game = self.require_game()?;
        if game.view.phase != GamePhase::Playing {
            return Ok(());
        }
        let scoring = game.scoring;
        if !scoring {
            match Box::pin(self.begin_coach_turn()).await {
                Ok(true) => {
                    self.publish_game();
                    return Ok(());
                }
                Ok(false) => {}
                Err(error) => {
                    self.fail_game_query(error.to_string()).await;
                    return Ok(());
                }
            }
        }
        let game = self.require_game()?;
        let gtp = match game.player(game.color()) {
            Player::Ai(crate::AiPlayer::Gtp { engine_id, time }) => {
                Some((engine_id.clone(), time.clone()))
            }
            _ => None,
        };
        if scoring && self.score_gtp_game().await? {
            self.publish_game();
            return Ok(());
        }
        if !scoring && let Some((id, time)) = gtp {
            if let Err(error) = self.submit_gtp_turn(id, Some(time)).await {
                self.finish_game(None, GameEndReason::EngineError).await?;
                self.record_game_error(error);
            }
            self.publish_game();
            return Ok(());
        }
        let game = self.require_game()?;
        let play = if scoring {
            None
        } else {
            match game.player(game.color()) {
                Player::Human
                | Player::Ai(crate::AiPlayer::Gtp { .. } | crate::AiPlayer::HumanSl(_)) => None,
                Player::Ai(crate::AiPlayer::KataGo(play)) => Some(play.clone()),
            }
        };
        if scoring || play.is_some() {
            let result = self.submit_game_query(play).await;
            if let Err(error) = result {
                if scoring {
                    self.finish_game(None, GameEndReason::Unscored).await?;
                } else {
                    self.fail_game_query(error.to_string()).await;
                }
            }
        } else {
            if let Some(game) = &mut self.game {
                if let Some(clock) = &mut game.clocks[index(game.color())] {
                    clock.start(Instant::now());
                }
                game.tick = Instant::now() + Duration::from_secs(1);
            }
            self.update_focus().await;
        }
        self.publish_game();
        Ok(())
    }
    async fn submit_game_query(&mut self, play: Option<KataGoPlay>) -> Result<(), AppError> {
        self.clear_game_live_query().await?;
        let engine = self.game_engine()?;
        let game = self.require_game()?;
        let node = NodeId(game.view.latest_node);
        let position = self.document.position_at(node)?;
        let (visits, seconds, pure) = play.as_ref().map_or((Some(200), None, false), |p| {
            (p.visits, p.seconds, p.pure_network)
        });
        let mut live = self.live_settings();
        // A play result must describe the actual SGF rules and komi.
        live.query_rules = None;
        live.query_komi = None;
        let mut request = analysis::play_request(&position, &live, visits, seconds, pure)?;
        self.match_override_settings(&mut request);
        let subscription = engine.submit(request).await?;
        let deadline = Instant::now()
            + seconds.map(Duration::from_secs_f64).unwrap_or_default()
            + self.config.game_query_timeout;
        if let Some(game) = &mut self.game {
            game.query = Some(Query {
                subscription,
                engine,
                node,
                key: position.analysis_key(),
                play,
                deadline,
                latest: None,
            });
        }
        Ok(())
    }
    pub(super) async fn update_game_clock(&mut self) -> Result<bool, AppError> {
        if !self.game_playing() {
            return Ok(false);
        }
        let expired = if let Some(game) = &mut self.game {
            let color = game.color();
            game.clocks[index(color)]
                .as_mut()
                .is_some_and(|clock| clock.update(Instant::now()))
                .then_some(color)
        } else {
            None
        };
        if let Some(color) = expired {
            self.finish_game(
                Some(results::win(opponent(color), "T")),
                GameEndReason::Timeout,
            )
            .await?;
            return Ok(true);
        }
        Ok(false)
    }
    pub(in crate::actor) async fn game_signal(&mut self, event: Event) {
        let result = self.handle_game_signal(event).await;
        if let Err(error) = result {
            if self.game_playing() && self.game.as_ref().is_some_and(|game| game.scoring) {
                if let Err(error) = self.finish_game(None, GameEndReason::Unscored).await {
                    self.record_game_error(error);
                }
                return;
            }
            if self.game_playing() && self.match_active() {
                let _ = self.finish_game(None, GameEndReason::EngineError).await;
            }
            if self.game_playing() {
                let _ = self.cancel_game_query().await;
                if let Some(game) = &mut self.game {
                    game.view.phase = GamePhase::Paused;
                }
            }
            self.record_game_error(error);
        }
    }
    async fn handle_game_signal(&mut self, event: Event) -> Result<(), AppError> {
        if !self.game_playing() {
            return Ok(());
        }
        let event = match event {
            Event::Query(event) => *event,
            Event::Gtp(outcome) => return self.receive_gtp_answer(*outcome).await,
            Event::GtpFailed(error) => {
                self.finish_game(None, GameEndReason::EngineError).await?;
                self.record_game_error(error.into());
                return Ok(());
            }
            Event::Tick => {
                if self.update_game_clock().await? {
                    return Ok(());
                }
                let expired = self
                    .game
                    .as_ref()
                    .and_then(|game| game.query.as_ref())
                    .is_some_and(|query| Instant::now() >= query.deadline);
                if expired {
                    self.game_query_failed("query timed out".into()).await?;
                }
                if let Some(game) = &mut self.game {
                    game.tick = Instant::now() + Duration::from_secs(1);
                }
                self.publish_game();
                return Ok(());
            }
        };
        match event {
            Some(QueryEvent::Partial(response)) => {
                // A visits-bounded search may legitimately run for many minutes.
                // Only silence consumes the AI safety budget; scoring keeps its
                // fixed deadline so a stream of partials cannot postpone the end.
                if let Some(query) = self.game.as_mut().and_then(|game| game.query.as_mut())
                    && query.play.is_some()
                {
                    query.deadline = Instant::now() + self.config.game_query_timeout;
                }
                if let Some(query) = self.game.as_mut().and_then(|game| game.query.as_mut()) {
                    query.latest = Some(response.clone());
                }
                self.publish_game_analysis(response)?;
            }
            Some(QueryEvent::Final(response)) => {
                self.validate_game_position()?;
                self.publish_game_analysis(response.clone())?;
                let Some(query) = self.game.as_mut().and_then(|game| game.query.take()) else {
                    return Ok(());
                };
                if let Some(play) = query.play {
                    if self.ai_resigns(&play, &response) {
                        let color = self.require_game()?.color();
                        self.finish_game(
                            Some(results::win(opponent(color), "R")),
                            GameEndReason::Resignation,
                        )
                        .await?;
                    } else {
                        let mv = self.select_game_move(&play, &response)?;
                        self.append_game_move(mv, false).await?;
                    }
                } else {
                    let komi = self.document.position_at(query.node)?.komi;
                    let result = crate::game::score_result(response.root_info.score_lead, komi);
                    let reason = if result.is_some() {
                        GameEndReason::Scored
                    } else {
                        GameEndReason::Unscored
                    };
                    self.finish_game(result, reason).await?;
                }
            }
            Some(QueryEvent::Restarted) => {
                if self.match_active() {
                    self.game_query_failed("engine restarted during match".into())
                        .await?;
                }
            }
            Some(QueryEvent::Failed(error)) => self.game_query_failed(error.to_string()).await?,
            Some(QueryEvent::Terminated) | None => {
                self.game_query_failed("query ended without a result".into())
                    .await?
            }
        }
        Ok(())
    }
    fn publish_game_analysis(&mut self, response: AnalysisResponse) -> Result<(), AppError> {
        if !self.remote_identity_current() {
            return Ok(());
        }
        if self.coach_hides_analysis() {
            return Ok(());
        }
        let game = self.require_game()?;
        let query = game.query.as_ref().ok_or(GameError::NotActive)?;
        let position = self.document.position_at(query.node)?;
        if query.node.0 != game.view.latest_node
            || position.analysis_key() != query.key
            || query.key != game.key
        {
            return Err(GameError::PositionChanged.into());
        }
        let snapshot = analysis::AnalysisSnapshot {
            revision: self.revision,
            node_id: query.node,
            key: query.key,
            response,
            source: analysis::SnapshotSource::Live,
            visits_per_second: None,
        };
        let mut view = AnalysisView::from_snapshot(
            snapshot.clone(),
            position.board.size(),
            position.board.to_play(),
        )
        .map_err(|error| GameError::Engine(error.to_string()))?;
        self.retain_analysis(&snapshot);
        let latest = view.summary;
        view.summary = *self
            .summaries
            .entry(snapshot.node_id)
            .and_modify(|best| {
                if latest.visits >= best.visits {
                    *best = latest;
                }
            })
            .or_insert(latest);
        if !self.match_active() {
            self.persist_analysis(&snapshot);
        }
        if self.document.current() == snapshot.node_id {
            self.output.send(AppEvent::AnalysisUpdated(view));
        } else {
            self.publish_document()?;
        }
        Ok(())
    }
    async fn game_query_failed(&mut self, reason: String) -> Result<(), AppError> {
        if self.require_game()?.scoring {
            self.finish_game(None, GameEndReason::Unscored).await
        } else {
            self.fail_game_query(reason).await;
            Ok(())
        }
    }
    pub(in crate::actor) async fn fail_game_query(&mut self, reason: String) {
        if self.match_active() {
            let _ = self.finish_game(None, GameEndReason::EngineError).await;
            if let Some(game) = &mut self.game {
                game.view.error = Some(GameError::Engine(reason));
            }
            self.publish_game();
            return;
        }
        let _ = self.cancel_game_query().await;
        if let Some(game) = &mut self.game {
            game.view.phase = GamePhase::Paused;
            game.view.error = Some(GameError::Engine(reason));
            for clock in game.clocks.iter_mut().flatten() {
                clock.pause(Instant::now());
            }
        }
        self.publish_game();
    }
    fn record_game_error(&mut self, error: AppError) {
        let error = match error {
            AppError::Game(error) => error,
            other => GameError::Engine(other.to_string()),
        };
        if let Some(game) = &mut self.game {
            game.view.error = Some(error);
        }
        self.publish_game();
    }
}
