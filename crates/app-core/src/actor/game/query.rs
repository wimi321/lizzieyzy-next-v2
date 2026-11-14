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
}
pub(in crate::actor) enum Event {
    Query(Box<Option<QueryEvent>>),
    Tick,
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
    tokio::select! {
        event = async { match &mut game.query {Some(query)=>query.subscription.recv().await,None=>pending().await} } => Event::Query(Box::new(event)),
        () = async {match deadline {Some(deadline)=>tokio::time::sleep_until(deadline).await,None=>pending().await}} => Event::Tick,
    }
}
impl Actor {
    pub(super) fn game_engine(&self) -> Result<EngineHandle, GameError> {
        let running = self.running.as_ref().ok_or(GameError::NoEngine)?;
        if matches!(
            *running.engine_status.borrow(),
            engine_session::EngineStatus::Failed { .. } | engine_session::EngineStatus::Stopped
        ) {
            return Err(GameError::NoEngine);
        }
        Ok(running.engine.clone())
    }
    pub(super) async fn clear_game_live_query(&mut self) -> Result<(), AppError> {
        self.flush_cached_focus().await;
        if let Some(running) = &self.running {
            running.coordinator.clear_focus().await?;
        }
        self.focus = None;
        self.output.clear_analysis();
        Ok(())
    }
    pub(in crate::actor) async fn cancel_game_query(&mut self) -> Result<(), AppError> {
        if let Some(query) = self.game.as_mut().and_then(|game| game.query.take()) {
            query
                .engine
                .terminate(query.subscription.id().to_owned())
                .await?;
        }
        Ok(())
    }
    pub(super) async fn begin_game_turn(&mut self) -> Result<(), AppError> {
        let game = self.require_game()?;
        if game.view.phase != GamePhase::Playing {
            return Ok(());
        }
        let scoring = game.scoring;
        let play = if scoring {
            None
        } else {
            match game.player(game.color()) {
                Player::Human => None,
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
        let request = analysis::play_request(&position, &live, visits, seconds, pure)?;
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
            Some(QueryEvent::Restarted) => {}
            Some(QueryEvent::Failed(error)) => self.game_query_failed(error.to_string()).await?,
            Some(QueryEvent::Terminated) | None => {
                self.game_query_failed("query ended without a result".into())
                    .await?
            }
        }
        Ok(())
    }
    fn publish_game_analysis(&mut self, response: AnalysisResponse) -> Result<(), AppError> {
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
        self.persist_analysis(&snapshot);
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
    async fn fail_game_query(&mut self, reason: String) {
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
