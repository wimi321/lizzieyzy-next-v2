use super::*;
use crate::{AiPlayer, PvPick};

impl Actor {
    pub(in crate::actor) async fn set_ai_player(
        &mut self,
        color: Color,
        player: AiPlayer,
    ) -> Result<Option<GameView>, AppError> {
        player.validate()?;
        if let Some(game) = &self.game
            && game.view.phase != GamePhase::Finished
            && matches!(game.player(color), Player::Human)
        {
            return Err(GameError::InvalidSetup(
                "only an existing AI player can be changed during a game".into(),
            )
            .into());
        }
        let mut settings = self.settings.clone();
        *settings.game_setup.player_mut(color) = Player::Ai(player.clone());
        settings.game_setup.validate()?;
        if let Some(game) = &self.game
            && game.view.phase != GamePhase::Finished
        {
            let mut setup = game.setup.clone();
            *setup.player_mut(color) = Player::Ai(player.clone());
            setup.validate()?;
        }
        settings.save(&self.directory).await?;
        self.settings = settings;
        if let Some(game) = &mut self.game
            && game.view.phase != GamePhase::Finished
        {
            // The in-flight query keeps its own captured play settings.
            *game.setup.player_mut(color) = Player::Ai(player);
            game.view.black = game.setup.black.clone();
            game.view.white = game.setup.white.clone();
        }
        self.publish_game();
        Ok(self.game_view())
    }
    pub(in crate::actor) async fn play_best_move(
        &mut self,
        pick: Option<PvPick>,
    ) -> Result<u64, AppError> {
        self.ensure_no_batch()?;
        if self.game_playing() {
            return Err(AppError::GameRunning);
        }
        let view = self
            .review_view(self.document.current().0)?
            .ok_or(GameError::NoAnalysis)?;
        let mut candidates = view.candidates;
        candidates.sort_by_key(|candidate| candidate.order);
        let command = match pick {
            None => {
                let candidate = candidates.first().ok_or(GameError::NoAnalysis)?;
                let color = self
                    .document
                    .position_at(self.document.current())?
                    .board
                    .to_play();
                document::Command::Play(match candidate.point {
                    Some(point) => Move::Play(color, point.into()),
                    None => Move::Pass(color),
                })
            }
            Some(pick) => {
                let candidate = candidates.get(pick.candidate).ok_or(GameError::InvalidPv)?;
                if pick.moves == 0 || pick.moves > candidate.pv.len() {
                    return Err(GameError::InvalidPv.into());
                }
                document::Command::AddVariation(
                    candidate
                        .pv
                        .iter()
                        .take(pick.moves)
                        .cloned()
                        .map(Into::into)
                        .collect(),
                )
            }
        };
        self.apply(super::super::Input::Edit(command)).await
    }
    pub(in crate::actor) async fn pause_game(&mut self) -> Result<(), AppError> {
        self.require_game()?;
        if !self.game_playing() {
            return Ok(());
        }
        if self.update_game_clock().await? {
            return Ok(());
        }
        self.cancel_game_query().await?;
        self.clear_game_live_query().await?;
        if let Some(game) = &mut self.game {
            for clock in game.clocks.iter_mut().flatten() {
                clock.pause(Instant::now());
            }
            game.view.phase = GamePhase::Paused;
        }
        self.publish_game();
        // Ordinary browsing and analysis are available while paused.
        self.update_focus().await;
        Ok(())
    }
    pub(in crate::actor) async fn resume_game(&mut self) -> Result<(), AppError> {
        let game = self.require_game()?;
        if game.view.phase != GamePhase::Paused {
            return Err(GameError::NotPaused.into());
        }
        self.validate_game_position()?;
        self.ensure_no_batch()?;
        self.ensure_no_pure_network()?;
        if !self.can_reuse_saved_analysis() {
            return Err(GameError::IncompatibleMode.into());
        }
        self.ensure_no_benchmark()?;
        self.ensure_no_move_trial()?;
        if self.tsumego.is_some() {
            return Err(GameError::IncompatibleMode.into());
        }
        self.stop_board_sync().await?;
        self.cancel_auto_curve_intent();
        self.cancel_review().await;
        self.clear_game_live_query().await?;
        if let Some(game) = &mut self.game {
            game.view.phase = GamePhase::Playing;
            game.view.error = None;
        }
        self.begin_game_turn().await
    }
    pub(in crate::actor) async fn resign_game(&mut self) -> Result<(), AppError> {
        let game = self.require_game()?;
        let human = match (&game.setup.black, &game.setup.white) {
            (Player::Human, Player::Human) => game.color(),
            (Player::Human, _) => Color::Black,
            (_, Player::Human) => Color::White,
            _ => return Err(GameError::NoHumanPlayer.into()),
        };
        self.finish_game(
            Some(results::win(opponent(human), "R")),
            GameEndReason::Resignation,
        )
        .await
    }
    pub(in crate::actor) async fn take_back(&mut self) -> Result<(), AppError> {
        self.validate_game_position()?;
        let game = self.require_game()?;
        let from = game
            .history
            .iter()
            .rposition(|turn| turn.human)
            .ok_or(GameError::CannotTakeBack)?;
        let first = game.history[from].node;
        let parent = game.history[from].parent;
        self.next_revision()?;
        if self.update_game_clock().await? {
            return Err(GameError::NotActive.into());
        }
        self.cancel_game_query().await?;
        self.clear_game_live_query().await?;
        self.document.apply(document::Command::DeleteNode(first))?;
        self.document.apply(document::Command::GoTo(parent))?;
        self.file_state.analysis_edits.document_edit();
        let position = self.document.position_at(parent)?;
        if let Some(game) = &mut self.game {
            for clock in game.clocks.iter_mut().flatten() {
                clock.finish_turn(Instant::now());
            }
            game.history.truncate(from);
            game.view.latest_node = parent.0;
            game.view.to_play = position.board.to_play().into();
            game.view.move_number = position.moves.len() as u32;
            game.key = position.analysis_key();
            game.low_winrates = [0; 2];
            game.scoring = false;
        }
        self.undo_gtp_players().await?;
        self.document_changed().await?;
        if self.game_playing() {
            self.begin_game_turn().await?;
        } else {
            self.publish_game();
        }
        Ok(())
    }
}
