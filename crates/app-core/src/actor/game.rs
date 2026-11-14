//! A play session borrows the existing engine, and owns at most one query.
use super::Actor;
use crate::{
    AppError, AppEvent, GameEndReason, GameError, GamePhase, GameSetup, GameStart, GameView,
    Player, game::ClockState, game_api::Request,
};
use document::{AnalysisKey, GameDocument, NodeId};
use go_core::{Color, Move};
use tokio::time::Instant;

mod control;
mod guard;
mod query;
mod results;
use query::Query;
pub(super) use query::receive;

pub(super) struct Session {
    setup: GameSetup,
    starting: bool,
    view: GameView,
    key: AnalysisKey,
    clocks: [Option<ClockState>; 2],
    history: Vec<Turn>,
    query: Option<Query>,
    scoring: bool,
    invalidated: bool,
    low_winrates: [u32; 2],
    random: u64,
    tick: Instant,
}
struct Turn {
    node: NodeId,
    parent: NodeId,
    human: bool,
}
fn index(color: Color) -> usize {
    if color == Color::Black { 0 } else { 1 }
}
fn opponent(color: Color) -> Color {
    if color == Color::Black {
        Color::White
    } else {
        Color::Black
    }
}
impl Session {
    fn player(&self, color: Color) -> &Player {
        if color == Color::Black {
            &self.setup.black
        } else {
            &self.setup.white
        }
    }
    fn color(&self) -> Color {
        self.view.to_play.into()
    }
    fn snapshot(&self) -> GameView {
        let mut view = self.view.clone();
        view.black_clock = self.clocks[0]
            .as_ref()
            .map(|clock| clock.view(Instant::now()));
        view.white_clock = self.clocks[1]
            .as_ref()
            .map(|clock| clock.view(Instant::now()));
        view.ai_thinking = self.query.is_some();
        view
    }
}
impl Actor {
    pub(super) fn game_view(&self) -> Option<GameView> {
        self.game.as_ref().map(Session::snapshot)
    }
    pub(super) fn game_playing(&self) -> bool {
        self.game
            .as_ref()
            .is_some_and(|game| game.view.phase == GamePhase::Playing)
    }
    pub(super) fn game_owns_query(&self) -> bool {
        self.game_playing()
            && self.game.as_ref().is_some_and(|game| {
                game.starting
                    || !game.setup.ponder
                    || game.scoring
                    || matches!(game.player(game.color()), Player::Ai(_))
            })
    }
    pub(super) fn game_focus_node(&self) -> Option<NodeId> {
        self.game
            .as_ref()
            .filter(|_| self.game_playing())
            .map(|game| NodeId(game.view.latest_node))
    }
    pub(super) fn game_pondering(&self) -> Option<bool> {
        self.game
            .as_ref()
            .filter(|_| self.game_playing())
            .map(|game| {
                !game.starting
                    && !game.scoring
                    && matches!(game.player(game.color()), Player::Human)
                    && game.setup.ponder
            })
    }
    fn publish_game(&self) {
        if let Some(view) = self.game_view() {
            self.output.send(AppEvent::GameUpdated(view));
        }
    }
    pub(super) fn invalidate_game_document(&mut self) {
        if let Some(game) = &mut self.game {
            game.invalidated = true;
        }
    }
    pub(super) async fn game_command(&mut self, request: Request) -> Result<GameView, AppError> {
        match request {
            Request::Start(setup) => self.start_game(*setup, true).await?,
            Request::Continue(color) => {
                let mut setup = self.settings.game_setup.clone();
                setup.start = GameStart::CurrentPosition;
                let color = color.unwrap_or(
                    self.document
                        .position_at(self.document.current())?
                        .board
                        .to_play(),
                );
                let player = match if color == Color::Black {
                    &setup.black
                } else {
                    &setup.white
                } {
                    Player::Ai(player) => player.clone(),
                    Player::Human => match [&setup.black, &setup.white].into_iter().find_map(|p| {
                        if let Player::Ai(ai) = p {
                            Some(ai.clone())
                        } else {
                            None
                        }
                    }) {
                        Some(player) => player,
                        None => crate::AiPlayer::KataGo(Default::default()),
                    },
                };
                setup.black = Player::Human;
                setup.white = Player::Human;
                if color == Color::Black {
                    setup.black = Player::Ai(player);
                } else {
                    setup.white = Player::Ai(player);
                }
                self.start_game(setup, false).await?;
            }
            Request::Pause => self.pause_game().await?,
            Request::Resume => self.resume_game().await?,
            Request::Stop => {
                self.require_game()?;
                self.finish_game(None, GameEndReason::Stopped).await?;
            }
            Request::Resign => self.resign_game().await?,
            Request::TakeBack => self.take_back().await?,
        }
        self.game_view().ok_or_else(|| GameError::NotActive.into())
    }
    fn require_game(&self) -> Result<&Session, GameError> {
        self.game
            .as_ref()
            .filter(|game| game.view.phase != GamePhase::Finished)
            .ok_or(GameError::NotActive)
    }
    fn validate_game_position(&self) -> Result<(), AppError> {
        let game = self.require_game()?;
        if game.invalidated
            || self
                .document
                .position_at(NodeId(game.view.latest_node))
                .is_err()
        {
            return Err(GameError::PositionChanged.into());
        }
        if self
            .document
            .position_at(NodeId(game.view.latest_node))?
            .analysis_key()
            != game.key
        {
            return Err(GameError::PositionChanged.into());
        }
        Ok(())
    }
    async fn start_game(&mut self, setup: GameSetup, persist_setup: bool) -> Result<(), AppError> {
        if self
            .game
            .as_ref()
            .is_some_and(|g| g.view.phase != GamePhase::Finished)
        {
            return Err(GameError::AlreadyActive.into());
        }
        setup.validate()?;
        self.ensure_no_batch()?;
        self.ensure_no_benchmark()?;
        self.ensure_no_move_trial()?;
        self.ensure_no_pure_network()?;
        if !self.can_reuse_saved_analysis() {
            return Err(GameError::IncompatibleMode.into());
        }
        if self.tsumego.is_some() {
            return Err(GameError::IncompatibleMode.into());
        }
        let document = match &setup.start {
            GameStart::NewBoard(options) => Some(GameDocument::new_game(options.clone().into())?),
            GameStart::CurrentPosition => None,
        };
        let position = match &document {
            Some(doc) => doc.position_at(doc.current())?,
            None => self.document.position_at(self.document.current())?,
        };
        if matches!(setup.black, Player::Ai(_)) || matches!(setup.white, Player::Ai(_)) {
            self.game_engine()?;
        }
        // Quick continuation only borrows the dialog's settings. Its current
        // position and chosen AI color belong to this session, not the defaults.
        if persist_setup {
            let mut settings = self.settings.clone();
            settings.game_setup = setup.clone();
            settings.save(&self.directory).await?;
            self.settings = settings;
        }
        self.stop_board_sync().await?;
        self.auto_setup.cancel();
        self.cancel_trial().await;
        self.cancel_auto_curve_intent();
        self.cancel_review().await;
        self.clear_game_live_query().await?;
        // Drop the old completed session before replacing the document namespace.
        self.game = None;
        let node = document
            .as_ref()
            .map_or(self.document.current(), GameDocument::current)
            .0;
        let to_play = position.board.to_play();
        let clocks = [&setup.black, &setup.white].map(|p| {
            if matches!(p, Player::Human) {
                setup.human_clock.map(ClockState::new)
            } else {
                None
            }
        });
        self.game = Some(Session {
            view: GameView {
                phase: GamePhase::Playing,
                black: setup.black.clone(),
                white: setup.white.clone(),
                to_play: to_play.into(),
                ai_thinking: false,
                black_clock: None,
                white_clock: None,
                move_number: position.moves.len() as u32,
                start_node: node,
                latest_node: node,
                result: None,
                end_reason: None,
                error: None,
            },
            setup,
            starting: true,
            clocks,
            key: position.analysis_key(),
            history: Vec::new(),
            query: None,
            scoring: false,
            invalidated: false,
            low_winrates: [0; 2],
            random: self.config.game_random_seed.unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |v| v.as_nanos() as u64)
            }),
            tick: Instant::now(),
        });
        if let Some(document) = document {
            self.replace_document(document).await?;
        }
        if let Some(game) = &mut self.game {
            game.starting = false;
            game.invalidated = false;
        }
        // A prior SGF result describes the old game, not this new continuation.
        self.set_game_result(None)?;
        self.document_changed().await?;
        self.begin_game_turn().await
    }
    pub(super) async fn game_human_move(&mut self, mv: Move) -> Result<u64, AppError> {
        self.validate_game_position()?;
        let game = self.require_game()?;
        if game.scoring || !matches!(game.player(game.color()), Player::Human) {
            return Err(GameError::AiTurn.into());
        }
        if self.document.current().0 != game.view.latest_node {
            return Err(GameError::NotLatestNode.into());
        }
        // Validate before spending the turn or clearing a valid pending query.
        let mut position = self.document.position_at(self.document.current())?;
        if mv.color() != position.board.to_play() {
            return Err(document::DocError::WrongPlayer {
                node: self.document.current(),
                expected: position.board.to_play(),
                actual: mv.color(),
            }
            .into());
        }
        position
            .board
            .play(mv)
            .map_err(|reason| document::DocError::IllegalMove {
                node: self.document.current(),
                reason,
            })?;
        if self.update_game_clock().await? {
            return Err(GameError::NotActive.into());
        }
        self.append_game_move(mv, true).await?;
        Ok(self.revision)
    }
    async fn append_game_move(&mut self, mv: Move, human: bool) -> Result<(), AppError> {
        self.validate_game_position()?;
        self.next_revision()?;
        let game = self.require_game()?;
        let parent = NodeId(game.view.latest_node);
        let browsing = self.document.current();
        let follow = browsing == parent;
        let color = game.color();
        self.clear_game_live_query().await?;
        self.document.apply(document::Command::GoTo(parent))?;
        self.document
            .apply(document::Command::PlaySequence(vec![mv]))?;
        let node = self.document.current();
        let position = self.document.position_at(node)?;
        self.file_state.analysis_edits.document_edit();
        if !follow {
            self.document.apply(document::Command::GoTo(browsing))?;
        }
        if let Some(game) = &mut self.game {
            if let Some(clock) = &mut game.clocks[index(color)] {
                clock.finish_turn(Instant::now());
            }
            game.history.push(Turn {
                node,
                parent,
                human,
            });
            game.view.latest_node = node.0;
            game.view.to_play = position.board.to_play().into();
            game.view.move_number = position.moves.len() as u32;
            game.key = position.analysis_key();
            game.scoring = position.moves.iter().rev().take(2).count() == 2
                && position
                    .moves
                    .iter()
                    .rev()
                    .take(2)
                    .all(|m| matches!(m, Move::Pass(_)));
        }
        self.document_changed().await?;
        self.begin_game_turn().await
    }
}
