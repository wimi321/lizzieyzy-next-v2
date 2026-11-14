use super::*;
use crate::{
    AiPlayer, CoachColor, GameError, GameSetup, GameStart, HumanSlPlay, NewGameOptions, Player,
};

impl Actor {
    pub(super) async fn prepare_coach(&mut self, setup: CoachSetup) -> Result<Prepared, AppError> {
        setup.validate()?;
        if self.coach.starting.is_some()
            || self
                .game_view()
                .is_some_and(|g| g.phase != GamePhase::Finished)
            || self.match_active()
        {
            return Err(GameError::AlreadyActive.into());
        }
        self.ensure_no_batch()?;
        self.ensure_no_benchmark()?;
        self.ensure_no_move_trial()?;
        self.ensure_no_pure_network()?;
        if self.tsumego.is_some() || !self.can_reuse_saved_analysis() {
            return Err(GameError::IncompatibleMode.into());
        }
        if setup.from_current_position {
            let position = self.document.position_at(self.document.current())?;
            if !position.board.stones().iter().any(Option::is_some) {
                return Err(CoachError::InvalidSetup(
                    "current position must contain stones".into(),
                )
                .into());
            }
        }
        let running = self.running.as_ref().ok_or(CoachError::NoEngine)?;
        let version = match running.engine_status.borrow().clone() {
            engine_session::EngineStatus::Ready { version } => version,
            _ => return Err(CoachError::NoEngine.into()),
        };
        if !crate::auto_setup::version::supports(&version, "1.15.0") {
            return Err(CoachError::UnsupportedVersion(version).into());
        }
        let mut profile = self.settings.engine.clone().ok_or(CoachError::NoEngine)?;
        match crate::assets::human_model_backend(&profile).await {
            crate::assets::HumanModelBackend::Supported
            | crate::assets::HumanModelBackend::Unknown => {}
            crate::assets::HumanModelBackend::TensorRt => {
                return Err(CoachError::TensorRtUnsupported.into());
            }
        }
        let assets = self.asset_handle()?;
        let path = match assets
            .verified_humansl_at(profile.human_model.clone())
            .await
        {
            Ok(path) => path,
            Err(_) => {
                let corrupt = if let Some(path) = &profile.human_model {
                    path.exists()
                } else {
                    matches!(
                        assets.humansl_status().await,
                        Ok(crate::HumanSlModelStatus::Corrupt)
                    )
                };
                return Err(if corrupt {
                    CoachError::ModelCorrupt
                } else {
                    CoachError::ModelMissing
                }
                .into());
            }
        };
        // Reuse the sole main owner. Ordinary startup never requires this file.
        let (restart, restore) = if profile.human_model.as_ref() != Some(&path)
            || profile.fingerprint().await.ok().as_ref()
                != self.running.as_ref().map(|r| &r.fingerprint)
        {
            let restore = profile.human_model.is_none().then(|| profile.clone());
            profile.human_model = Some(path);
            (Some(profile), restore)
        } else {
            (None, None)
        };
        Ok(Prepared {
            setup,
            restart,
            restore,
        })
    }
    pub(super) async fn activate_coach(
        &mut self,
        setup: CoachSetup,
    ) -> Result<CoachView, AppError> {
        let profile = self.settings.engine.clone().ok_or(CoachError::NoEngine)?;
        let path = profile
            .human_model
            .clone()
            .ok_or(CoachError::ModelMissing)?;
        self.asset_handle()?
            .verified_humansl_at(Some(path.clone()))
            .await
            .map_err(|_| {
                if path.exists() {
                    CoachError::ModelCorrupt
                } else {
                    CoachError::ModelMissing
                }
            })?;
        if profile.fingerprint().await.ok().as_ref()
            != self.running.as_ref().map(|r| &r.fingerprint)
        {
            return Err(CoachError::ModelCorrupt.into());
        }
        let mut random = self.config.game_random_seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos() as u64)
        });
        let human_color = if setup.handicap > 0 {
            Color::Black
        } else {
            match setup.color {
                CoachColor::Black => Color::Black,
                CoachColor::White => Color::White,
                CoachColor::Random => {
                    if crate::game::random_unit(&mut random) < 0.5 {
                        Color::Black
                    } else {
                        Color::White
                    }
                }
            }
        };
        let ai = Player::Ai(AiPlayer::HumanSl(HumanSlPlay {
            rank: setup.rank,
            pace: setup.pace,
        }));
        let start = if setup.from_current_position {
            GameStart::CurrentPosition
        } else {
            GameStart::NewBoard(NewGameOptions {
                width: setup.width,
                height: setup.height,
                komi: setup.komi(),
                handicap: if setup.handicap == 1 {
                    0
                } else {
                    setup.handicap
                },
                ..Default::default()
            })
        };
        let game = GameSetup {
            black: if human_color == Color::Black {
                Player::Human
            } else {
                ai.clone()
            },
            white: if human_color == Color::White {
                Player::Human
            } else {
                ai
            },
            start,
            human_clock: None,
            ponder: setup.mode == CoachMode::LiveAnalysis,
            auto_save: setup.auto_save,
        };
        let view = CoachView {
            active: true,
            setup: setup.clone(),
            human_color: human_color.into(),
            awaiting_decision: false,
            hints: 0,
            take_backs: 0,
            last_rating: None,
            summary: None,
        };
        self.coach.session = Some(Session {
            view,
            active: true,
            ai: None,
            review: None,
            before: None,
            rating: None,
            ratings: Vec::new(),
            hints: HashMap::new(),
            low_winrates: 0,
        });
        if let Err(error) = Box::pin(self.start_game(game, false)).await {
            self.cancel_coach_queries().await;
            self.coach.session = None;
            self.update_focus().await;
            return Err(error);
        }
        if let Some(game) = self.game_view()
            && let Some(error) = game.error
        {
            return Err(error.into());
        }
        // A failed start never changes the remembered setup.
        let mut settings = self.settings.clone();
        settings.coach_setup = setup.persisted();
        if let Err(error) = settings.save(&self.directory).await {
            self.pause_game().await?;
            return Err(error.into());
        }
        self.settings = settings;
        self.publish_coach();
        self.coach_view()
            .ok_or_else(|| CoachError::NotActive.into())
    }
    pub(in crate::actor) fn finish_coach_summary(&mut self) {
        let Some(game) = self.game_view() else {
            return;
        };
        let Some(session) = &mut self.coach.session else {
            return;
        };
        if !session.active || session.view.summary.is_some() {
            return;
        }
        session.view.awaiting_decision = false;
        let summary = crate::CoachSummary::from_ratings(
            game.result,
            game.end_reason,
            game.move_number,
            session.view.hints,
            session.view.take_backs,
            &session.ratings,
        );
        session.view.summary = Some(summary.clone());
        self.output.send(AppEvent::CoachSummary(summary));
        self.publish_coach();
    }
    pub(super) async fn coach_review(&mut self) -> Result<(), AppError> {
        self.coach_session()?;
        let latest = self.game_view().ok_or(CoachError::NotActive)?.latest_node;
        if self
            .game_view()
            .is_some_and(|g| g.phase != GamePhase::Finished)
        {
            self.finish_game(None, crate::GameEndReason::Stopped)
                .await?;
        }
        self.cancel_coach_queries().await;
        if let Some(session) = &mut self.coach.session {
            session.active = false;
        }
        self.document
            .apply(document::Command::GoTo(NodeId(latest)))?;
        self.document_changed().await?;
        self.set_pondering(true).await?;
        self.update_focus().await;
        self.publish_coach();
        Ok(())
    }
    pub(in crate::actor) fn coach_took_back(&mut self) {
        if let Some(session) = &mut self.coach.session {
            session.view.take_backs = session.view.take_backs.saturating_add(1);
            session.view.awaiting_decision = false;
            session.view.last_rating = None;
            session.before = None;
            session.low_winrates = 0;
            // Deleted node IDs must never be offered by the post-game review.
            session
                .ratings
                .retain(|r| self.document.node(NodeId(r.node_id)).is_ok());
        }
        self.publish_coach();
    }
}
