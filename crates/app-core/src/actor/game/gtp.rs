use super::*;
use crate::{AiPlayer, GtpTime, game::GtpClock};
use document::GamePosition;
use engine_session::gtp::{GtpHandle, GtpInfo, GtpStatus};
use gtp_sync::{error, send};
use std::{future::Future, pin::Pin};

#[derive(Clone)]
pub(super) struct State {
    pub id: String,
    pub handle: GtpHandle,
    pub info: GtpInfo,
    pub moves: Vec<Move>,
    pub initialized: bool,
    pub last_time: Option<GtpTime>,
    pub clocks: [Option<GtpClock>; 2],
}
impl State {
    pub fn new(id: String, handle: GtpHandle, info: GtpInfo) -> Self {
        Self {
            id,
            handle,
            info,
            moves: Vec::new(),
            initialized: false,
            last_time: None,
            clocks: [None, None],
        }
    }
}
pub(super) struct Pending {
    pub future: Pin<Box<dyn Future<Output = Outcome> + Send + Sync>>,
    pub engine: GtpHandle,
    pub id: String,
    pub color: Color,
    pub started: tokio::sync::watch::Receiver<Option<Instant>>,
}
pub(in crate::actor) struct Outcome {
    state: State,
    node: NodeId,
    key: AnalysisKey,
    result: Result<Answer, GameError>,
}
enum Answer {
    Move(Move),
    Resign,
    Score(String),
}

async fn run(
    mut state: State,
    position: GamePosition,
    node: NodeId,
    time: Option<GtpTime>,
    started: tokio::sync::watch::Sender<Option<Instant>>,
    response_timeout: std::time::Duration,
) -> Outcome {
    let key = position.analysis_key();
    let color = position.board.to_play();
    let result = async {
        gtp_sync::sync(&mut state, &position).await?;
        let Some(time) = time else {
            let body = send(&state.handle, &state.id, "final_score").await?;
            let score =
                final_score(&body).ok_or_else(|| error(&state.id, "invalid final_score reply"))?;
            return Ok(Answer::Score(score));
        };
        gtp_sync::set_time(&mut state, color, &time).await?;
        if let Some(text) = state.clocks[index(color)]
            .as_ref()
            .and_then(|clock| clock.time_left(color))
        {
            send(&state.handle, &state.id, text).await?;
        }
        let start = Instant::now();
        started.send_replace(Some(start));
        let name = if color == Color::Black {
            "black"
        } else {
            "white"
        };
        let timeout = state.clocks[index(color)]
            .as_ref()
            .map_or(response_timeout, |clock| {
                clock.response_timeout(response_timeout)
            });
        let reply = state
            .handle
            .command_with_timeout(format!("genmove {name}"), timeout)
            .await
            .map(|response| response.body)
            .map_err(|e| error(&state.id, e))?;
        if let Some(clock) = &mut state.clocks[index(color)] {
            clock.spend(start.elapsed().as_secs_f64(), true);
        }
        let coordinate = reply.trim();
        let mv = match ::gtp::parse_genmove(coordinate, position.board.size())
            .map_err(|_| GameError::GtpIllegalMove(coordinate.into()))?
        {
            ::gtp::Genmove::Resign => return Ok(Answer::Resign),
            ::gtp::Genmove::Pass => Move::Pass(color),
            ::gtp::Genmove::Play(point) => Move::Play(color, point),
        };
        let mut board = position.board;
        board
            .play(mv)
            .map_err(|e| GameError::GtpIllegalMove(format!("{coordinate}: {e}")))?;
        state.moves.push(mv);
        Ok(Answer::Move(mv))
    }
    .await;
    Outcome {
        state,
        node,
        key,
        result,
    }
}

fn final_score(body: &str) -> Option<String> {
    let text = body.trim();
    if text == "0" || text.eq_ignore_ascii_case("draw") || text.eq_ignore_ascii_case("jigo") {
        return Some("0".into());
    }
    let (winner, amount) = text.split_once('+')?;
    if !winner.eq_ignore_ascii_case("b") && !winner.eq_ignore_ascii_case("w") {
        return None;
    }
    let amount = if amount.eq_ignore_ascii_case("r") || amount.eq_ignore_ascii_case("resign") {
        "R".to_owned()
    } else {
        let value = amount.parse::<f64>().ok()?;
        if !value.is_finite() || value <= 0.0 {
            return None;
        }
        value.to_string()
    };
    Some(format!("{}+{amount}", winner.to_ascii_uppercase()))
}

impl Actor {
    pub(in crate::actor) fn game_gtp_engine_busy(&self, id: &str) -> bool {
        self.game.as_ref().is_some_and(|game| {
            game.view.phase != GamePhase::Finished && (game.gtp_query.as_ref().is_some_and(|q| q.id == id) || [&game.setup.black, &game.setup.white].iter().any(|p| {
                matches!(p, Player::Ai(AiPlayer::Gtp { engine_id, .. }) if engine_id == id)
            }))
        })
    }

    pub(super) async fn submit_gtp_turn(
        &mut self,
        id: String,
        time: Option<GtpTime>,
    ) -> Result<(), AppError> {
        self.clear_game_live_query().await?;
        let game = self.require_game()?;
        let node = NodeId(game.view.latest_node);
        let position = self.document.position_at(node)?;
        let color = position.board.to_play();
        let mut state = game.gtp_players.get(&id).cloned();
        if let Some(state) = &state {
            match state.handle.status().borrow().clone() {
                GtpStatus::Failed(reason) => return Err(error(&id, reason).into()),
                GtpStatus::Stopped if state.initialized => {
                    return Err(error(&id, "engine stopped during game").into());
                }
                _ => {}
            }
        }
        if state
            .as_ref()
            .is_none_or(|s| !matches!(*s.handle.status().borrow(), GtpStatus::Ready(_)))
        {
            let handle = self.get_gtp_handle(&id).await?;
            let info = handle.ready().await.map_err(|e| error(&id, e))?;
            let mut fresh = State::new(id.clone(), handle, info);
            if let Some(old) = state {
                fresh.clocks = old.clocks;
            }
            state = Some(fresh);
        }
        let mut state = state.ok_or(GameError::NoEngine)?;
        if let Some(time) = &time {
            gtp_sync::set_clock(&mut state, color, time);
        }
        // Capture a changed time control in the actor before genmove starts.
        // Cancellation charges this same clock even if the future has no reply.
        if let Some(game) = &mut self.game {
            game.gtp_players.insert(id.clone(), state.clone());
        }
        let engine = state.handle.clone();
        let (start_sender, started) = tokio::sync::watch::channel(None);
        let future = Box::pin(run(
            state,
            position,
            node,
            time,
            start_sender,
            self.config.gtp_session.response_timeout,
        ));
        if let Some(game) = &mut self.game {
            game.gtp_query = Some(Pending {
                future,
                engine,
                id,
                color,
                started,
            });
        }
        Ok(())
    }

    pub(super) async fn score_gtp_game(&mut self) -> Result<bool, AppError> {
        let scorer = self
            .require_game()?
            .gtp_players
            .values()
            .find(|s| s.info.supports("final_score"))
            .map(|s| s.id.clone());
        if let Some(id) = scorer {
            self.submit_gtp_turn(id, None).await?;
            return Ok(true);
        }
        if self.game_engine().is_err() && !self.require_game()?.gtp_players.is_empty() {
            self.finish_game(None, GameEndReason::Unscored).await?;
            if let Some(game) = &mut self.game {
                game.view.error = Some(GameError::Unscored);
            }
            self.publish_game();
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) async fn receive_gtp_answer(&mut self, outcome: Outcome) -> Result<(), AppError> {
        self.validate_game_position()?;
        let game = self.require_game()?;
        if game.key != outcome.key || game.view.latest_node != outcome.node.0 {
            return Err(GameError::PositionChanged.into());
        }
        if let Some(game) = &mut self.game {
            game.gtp_query = None;
            game.gtp_players
                .insert(outcome.state.id.clone(), outcome.state);
        }
        match outcome.result {
            Ok(Answer::Move(mv)) => self.append_game_move(mv, false).await,
            Ok(Answer::Resign) => {
                let color = self.require_game()?.color();
                self.finish_game(
                    Some(results::win(opponent(color), "R")),
                    GameEndReason::Resignation,
                )
                .await
            }
            Ok(Answer::Score(score)) => self.finish_game(Some(score), GameEndReason::Scored).await,
            Err(error) => {
                let scoring = self.require_game()?.scoring;
                self.finish_game(
                    None,
                    if scoring {
                        GameEndReason::Unscored
                    } else {
                        GameEndReason::EngineError
                    },
                )
                .await?;
                if let Some(game) = &mut self.game {
                    game.view.error = Some(error);
                }
                self.publish_game();
                Ok(())
            }
        }
    }

    pub(super) async fn cancel_gtp_query(&mut self) -> Result<(), AppError> {
        if let Some(pending) = self.game.as_mut().and_then(|game| game.gtp_query.take()) {
            if let Some(state) = self
                .game
                .as_mut()
                .and_then(|game| game.gtp_players.get_mut(&pending.id))
            {
                if let Some(clock) = &mut state.clocks[index(pending.color)]
                    && let Some(started) = *pending.started.borrow()
                {
                    clock.spend(started.elapsed().as_secs_f64(), false);
                }
                state.initialized = false;
            }
            // Standard GTP has no cancellation command. Reap this process before
            // resuming with a fresh one, so a late genmove cannot corrupt the board.
            pending
                .engine
                .shutdown()
                .await
                .map_err(|e| error(&pending.id, e))?;
        }
        Ok(())
    }
}

pub(super) async fn failure(
    states: Vec<(String, tokio::sync::watch::Receiver<GtpStatus>, bool)>,
) -> GameError {
    let mut states = states.into_iter();
    tokio::select! {
        error = wait_failure(states.next()) => error,
        error = wait_failure(states.next()) => error,
    }
}
async fn wait_failure(
    state: Option<(String, tokio::sync::watch::Receiver<GtpStatus>, bool)>,
) -> GameError {
    let Some((id, mut status, initialized)) = state else {
        return std::future::pending().await;
    };
    loop {
        let current = status.borrow_and_update().clone();
        match current {
            GtpStatus::Failed(reason) => return error(&id, reason),
            GtpStatus::Stopped if initialized => return error(&id, "engine stopped during game"),
            GtpStatus::Stopped => return std::future::pending().await,
            _ => {}
        }
        if status.changed().await.is_err() {
            return error(&id, "engine status closed");
        }
    }
}
