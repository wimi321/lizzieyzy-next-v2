use super::*;
impl Actor {
    pub(in crate::actor) async fn match_command(
        &mut self,
        request: Request,
    ) -> Result<MatchView, AppError> {
        match request {
            Request::Start(setup) => self.start_match(*setup).await?,
            Request::Pause => {
                self.require_match()?;
                if self
                    .match_session
                    .as_ref()
                    .is_some_and(|s| s.view.phase == MatchPhase::Running)
                {
                    if self
                        .game_view()
                        .is_some_and(|g| g.phase == GamePhase::Playing)
                    {
                        self.pause_game().await?;
                    }
                    if let Some(session) = &mut self.match_session {
                        session.view.phase = MatchPhase::Paused;
                    }
                    self.clear_game_live_query().await?;
                }
            }
            Request::Resume => {
                self.require_match()?;
                if !self
                    .match_session
                    .as_ref()
                    .is_some_and(|s| s.view.phase == MatchPhase::Paused)
                {
                    return Err(MatchError::NotPaused.into());
                }
                if let Some(session) = &mut self.match_session {
                    session.view.phase = MatchPhase::Running;
                }
                if self
                    .game_view()
                    .is_some_and(|g| g.phase == GamePhase::Paused && g.error.is_none())
                    && let Err(error) = self.resume_game().await
                {
                    if let Some(session) = &mut self.match_session {
                        session.view.phase = MatchPhase::Paused;
                    }
                    return Err(error);
                }
            }
            Request::Stop => {
                self.require_match()?;
                if self
                    .game_view()
                    .is_some_and(|g| g.phase != GamePhase::Finished)
                {
                    self.finish_game(None, GameEndReason::Stopped).await?;
                }
                if let Some(session) = &mut self.match_session {
                    session.view.phase = MatchPhase::Stopped;
                    session.pending_game = false;
                }
                self.shutdown_match().await;
            }
            Request::Games(games) => {
                self.require_match()?;
                if games > 1_000_000 {
                    return Err(
                        MatchError::InvalidSetup("game count exceeds 1000000".into()).into(),
                    );
                }
                if let Some(session) = &mut self.match_session {
                    // A reduction below the current ordinal means finish this game.
                    session.view.games = games.max(session.view.completed.saturating_add(1));
                    session.setup.games = session.view.games;
                }
            }
            Request::PlayNow => {
                self.require_match()?;
                let session = self.match_session.as_ref().ok_or(MatchError::NotActive)?;
                let color = self
                    .game_view()
                    .ok_or(MatchError::NotActive)?
                    .to_play
                    .into();
                if !matches!(
                    session.player(session.side_for_color(color)),
                    MatchPlayer::KataGo { .. }
                ) {
                    return Err(MatchError::NotKataGo.into());
                }
                self.game_play_now().await?;
            }
            Request::Resign(color) => {
                self.require_match()?;
                let result = if color == Color::Black { "W+R" } else { "B+R" };
                self.finish_game(Some(result.into()), GameEndReason::Resignation)
                    .await?;
            }
            Request::Params(side, wrn, pda) => {
                self.require_match()?;
                crate::match_game::validate_params(wrn, pda)?;
                let session = self.match_session.as_mut().ok_or(MatchError::NotActive)?;
                let player = match side {
                    MatchSide::A => &mut session.setup.a,
                    MatchSide::B => &mut session.setup.b,
                };
                match player {
                    MatchPlayer::KataGo {
                        wrn: old_wrn,
                        pda: old_pda,
                        ..
                    } => {
                        *old_wrn = wrn;
                        *old_pda = pda;
                    }
                    _ => return Err(MatchError::NotKataGo.into()),
                }
            }
        }
        self.publish_match();
        self.match_view()
            .ok_or_else(|| MatchError::NotActive.into())
    }
    fn require_match(&self) -> Result<(), AppError> {
        if self.match_active() {
            Ok(())
        } else {
            Err(MatchError::NotActive.into())
        }
    }
    async fn start_match(&mut self, setup: MatchSetup) -> Result<(), AppError> {
        if self.match_active() {
            return Err(MatchError::AlreadyActive.into());
        }
        if self
            .game_view()
            .is_some_and(|game| game.phase != GamePhase::Finished)
        {
            return Err(crate::GameError::AlreadyActive.into());
        }
        setup.validate()?;
        self.ensure_no_batch()?;
        self.ensure_no_benchmark()?;
        self.ensure_no_move_trial()?;
        self.ensure_no_pure_network()?;
        if self.tsumego.is_some() || !self.can_reuse_saved_analysis() {
            return Err(crate::GameError::IncompatibleMode.into());
        }
        let mut names = Vec::new();
        for player in [&setup.a, &setup.b] {
            let name = match player {
                MatchPlayer::KataGo { engine_id, .. } => self
                    .settings
                    .engines
                    .iter()
                    .find(|e| e.id == *engine_id)
                    .map(|e| e.name.clone()),
                MatchPlayer::Gtp { engine_id, .. } => self
                    .settings
                    .gtp_engines
                    .iter()
                    .find(|e| e.id == *engine_id)
                    .map(|e| e.name.clone()),
            }
            .ok_or_else(|| MatchError::EngineMissing(player.engine_id().into()))?;
            names.push(name);
        }
        let names = [names.remove(0), names.remove(0)];
        let current_seed = if matches!(setup.start, GameStart::CurrentPosition) {
            Some(self.document.trial_copy()?)
        } else {
            None
        };
        let directory = if setup.auto_save {
            let timestamp = self.config.batch_clock.local_timestamp();
            if timestamp.len() != 14 || !timestamp.bytes().all(|b| b.is_ascii_digit()) {
                return Err(MatchError::InvalidSetup("invalid match timestamp".into()).into());
            }
            Some(
                self.settings
                    .games_directory
                    .clone()
                    .unwrap_or_else(|| self.directory.join("MyGames"))
                    .join("EngineGames")
                    .join(format!(
                        "{}_VS_{}_{}",
                        crate::match_game::file_component(&names[0]),
                        crate::match_game::file_component(&names[1]),
                        timestamp
                    )),
            )
        } else {
            None
        };
        self.stop_board_sync().await?;
        self.cancel_auto_curve_intent();
        self.cancel_review().await;
        self.cancel_trial().await;
        self.clear_game_live_query().await?;
        let mut view = MatchView::new(setup.games);
        view.save_directory = directory.as_ref().map(|p| p.to_string_lossy().into_owned());
        self.match_session = Some(Session {
            view,
            setup,
            engines: BTreeMap::new(),
            owned_engines: BTreeSet::new(),
            gtp_engines: BTreeSet::new(),
            names,
            current_seed,
            directory,
            random: self.config.game_random_seed.unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64)
            }),
            pending_game: true,
            move_limit: 0,
            end_reason: None,
        });
        if let Err(error) = self.start_match_engines().await {
            self.shutdown_match().await;
            self.match_session = None;
            self.update_focus().await;
            return Err(error);
        }
        self.advance_match().await;
        Ok(())
    }
    async fn start_match_engines(&mut self) -> Result<(), AppError> {
        let session = self.match_session.as_ref().ok_or(MatchError::NotActive)?;
        let players = [session.setup.a.clone(), session.setup.b.clone()];
        for player in players {
            match player {
                MatchPlayer::Gtp { engine_id, .. } => {
                    if let Some(session) = &mut self.match_session {
                        session.gtp_engines.insert(engine_id);
                    }
                }
                MatchPlayer::KataGo { engine_id, .. } => {
                    if self
                        .match_session
                        .as_ref()
                        .is_some_and(|s| s.engines.contains_key(&engine_id))
                    {
                        continue;
                    }
                    let borrowed = if self.settings.main_engine_id.as_ref() == Some(&engine_id) {
                        self.running.as_ref().map(|r| r.engine.clone())
                    } else {
                        None
                    };
                    let owned = borrowed.is_none();
                    let engine = match borrowed {
                        Some(engine) => engine,
                        None => {
                            let profile = self
                                .settings
                                .engines
                                .iter()
                                .find(|e| e.id == engine_id)
                                .ok_or_else(|| MatchError::EngineMissing(engine_id.clone()))?
                                .profile
                                .clone();
                            let (transport, _) = self.prepare_engine_transport(&profile).await?;
                            let mut config = self.session_config();
                            config.max_restarts = 0;
                            engine_session::spawn(transport, config)
                        }
                    };
                    if let Some(session) = &mut self.match_session {
                        session.engines.insert(engine_id.clone(), engine);
                        if owned {
                            session.owned_engines.insert(engine_id);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
