use super::*;
impl Actor {
    pub(in crate::actor) async fn advance_match(&mut self) {
        if !self.match_needs_advance() {
            return;
        }
        let pending = self.match_session.as_ref().is_some_and(|s| s.pending_game);
        if !pending {
            if let Some(game) = self.game_view() {
                if game.phase == GamePhase::Paused && game.error.is_some() {
                    let reason = game
                        .error
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "engine failure".into());
                    if let Some(session) = &mut self.match_session {
                        session.end_reason = Some(reason);
                    }
                    if let Err(error) = self.finish_game(None, GameEndReason::Unscored).await {
                        self.match_error(error);
                    }
                }
                let game = self.game_view();
                if let Some(game) = game {
                    let reason = self
                        .match_session
                        .as_ref()
                        .and_then(|s| s.end_reason.clone())
                        .or_else(|| game.error.as_ref().map(ToString::to_string))
                        .unwrap_or_else(|| {
                            if game.result.is_none() {
                                "未数子".into()
                            } else {
                                "已计分".into()
                            }
                        });
                    if let Some(session) = &mut self.match_session {
                        session.view.record(game.result.as_deref(), &reason);
                    }
                    if let Err(error) = self.save_match_game(game.result.as_deref(), &reason).await
                    {
                        self.match_error(error.into());
                    }
                }
            }
            if let Some(session) = &mut self.match_session {
                session.pending_game = true;
            }
        }
        let complete = self
            .match_session
            .as_ref()
            .is_some_and(|s| s.view.completed >= s.view.games);
        if complete {
            if let Some(session) = &mut self.match_session {
                session.view.phase = MatchPhase::Completed;
                session.pending_game = false;
            }
            self.shutdown_match().await;
        } else if let Err(error) = self.start_match_game().await {
            let reason = error.to_string();
            if let Some(session) = &mut self.match_session {
                session.view.record(None, &reason);
                session.pending_game = true;
            }
            self.match_error(error);
        }
        self.publish_match();
    }
    fn match_error(&mut self, error: AppError) {
        let error = match error {
            AppError::Match(error) => error,
            other => MatchError::Engine(other.to_string()),
        };
        if let Some(session) = &mut self.match_session {
            session.view.error = Some(error);
        }
    }
    async fn start_match_game(&mut self) -> Result<(), AppError> {
        let session = self.match_session.as_mut().ok_or(MatchError::NotActive)?;
        session.view.current_game = session.view.completed + 1;
        session.view.a_is_black =
            !session.setup.swap_colors || session.view.completed.is_multiple_of(2);
        session.end_reason = None;
        session.view.current_opening = None;
        if session.setup.openings.is_empty()
            && let GameStart::FreeHandicap { options, stones } = &session.setup.start
        {
            let mut options = options.clone();
            let (black_name, white_name) = session.names();
            options.black_name = Some(black_name.into());
            options.white_name = Some(white_name.into());
            session.move_limit = session
                .setup
                .max_moves
                .unwrap_or(u32::from(options.width) * u32::from(options.height) * 2);
            let (black, white) = if session.view.a_is_black {
                (&session.setup.a, &session.setup.b)
            } else {
                (&session.setup.b, &session.setup.a)
            };
            let setup = GameSetup {
                black: Player::Ai(black.player(session.setup.random_opening)),
                white: Player::Ai(white.player(session.setup.random_opening)),
                start: GameStart::FreeHandicap {
                    options,
                    stones: *stones,
                },
                human_clock: None,
                ponder: false,
                auto_save: false,
            };
            session.pending_game = false;
            self.game = None;
            return self.start_game(setup, false).await;
        }
        let mut document = if !session.setup.openings.is_empty() {
            let index = if session.setup.random_openings {
                session.random = session.random.wrapping_add(0x9e3779b97f4a7c15);
                let mut value = session.random;
                value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
                value ^= value >> 31;
                (value % session.setup.openings.len() as u64) as usize
            } else {
                session.view.completed as usize % session.setup.openings.len()
            };
            let path = session.setup.openings[index].clone();
            session.view.current_opening = Some(path.to_string_lossy().into_owned());
            read_opening(&path).await?
        } else if let Some(document) = &session.current_seed {
            document.clone()
        } else {
            let GameStart::NewBoard(options) = &session.setup.start else {
                return Err(
                    MatchError::InvalidSetup("missing current-position seed".into()).into(),
                );
            };
            GameDocument::new_game(options.clone().into())?
        };
        let position = document.position_at(document.current())?;
        session.move_limit = session.setup.max_moves.unwrap_or(
            u32::from(position.board.size().width) * u32::from(position.board.size().height) * 2,
        );
        let (black_name, white_name) = session.names();
        let info = document.game_info()?;
        document.apply(document::Command::SetGameInfo(document::GameInfoEdit {
            black_name: Some(black_name.into()),
            white_name: Some(white_name.into()),
            black_rank: None,
            white_rank: None,
            result: None,
            komi: info.komi,
            rules: info.rules_preset,
            date: info.date,
            event: info.event,
        }))?;
        let (black, white) = if session.view.a_is_black {
            (&session.setup.a, &session.setup.b)
        } else {
            (&session.setup.b, &session.setup.a)
        };
        let setup = GameSetup {
            black: Player::Ai(black.player(session.setup.random_opening)),
            white: Player::Ai(white.player(session.setup.random_opening)),
            start: GameStart::CurrentPosition,
            human_clock: None,
            ponder: false,
            auto_save: false,
        };
        session.pending_game = false;
        self.game = None;
        self.replace_document(document).await?;
        self.start_game(setup, false).await?;
        Ok(())
    }
    async fn save_match_game(
        &mut self,
        result: Option<&str>,
        reason: &str,
    ) -> Result<(), MatchError> {
        let Some(session) = &self.match_session else {
            return Ok(());
        };
        let Some(directory) = session.directory.clone() else {
            return Ok(());
        };
        let (black, white) = session.names();
        let name = crate::match_game::match_file_name(
            session.view.current_game,
            black,
            white,
            result,
            reason,
        );
        let result = async {
            tokio::fs::create_dir_all(&directory).await?;
            let (tree, warnings) = self
                .export_document_with_analysis()
                .map_err(std::io::Error::other)?;
            let bytes = sgf::serialize(&sgf::Collection {
                games: vec![tree],
                encoding: "UTF-8".into(),
                warnings: Vec::new(),
            })
            .into_bytes();
            // Never overwrite an earlier run started in the same second.
            let mut suffix = 1_u32;
            loop {
                let file = if suffix == 1 {
                    name.clone()
                } else {
                    format!("{}-{suffix}.sgf", name.trim_end_matches(".sgf"))
                };
                let path = std::path::absolute(directory.join(file))?;
                match crate::storage::write_new(path.clone(), bytes.clone()).await {
                    Ok(()) => {
                        self.file_name = path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned());
                        self.file_state.mark_saved(&self.document, path);
                        self.file_state.save_warnings = warnings;
                        self.publish_document().map_err(std::io::Error::other)?;
                        return Ok::<(), std::io::Error>(());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        suffix = suffix
                            .checked_add(1)
                            .ok_or_else(|| std::io::Error::other("output collision limit"))?;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        .await;
        result.map_err(|error| MatchError::Save {
            path: directory.to_string_lossy().into_owned(),
            reason: error.to_string(),
        })
    }
    pub(in crate::actor) async fn shutdown_match(&mut self) {
        let (engines, gtp, played) = if let Some(session) = &mut self.match_session {
            let engines = std::mem::take(&mut session.owned_engines)
                .into_iter()
                .filter_map(|id| session.engines.remove(&id))
                .collect::<Vec<_>>();
            session.engines.clear();
            (
                engines,
                std::mem::take(&mut session.gtp_engines),
                session.view.current_game > 0,
            )
        } else {
            return;
        };
        for engine in engines {
            let _ = engine.shutdown().await;
        }
        for id in gtp {
            let _ = self.close_gtp_engine(&id).await;
        }
        self.clear_game_live_query().await.ok();
        if played {
            // Side-specific engines and overrides cannot seed the ordinary
            // main engine cache. Auto-save already exported this analysis.
            self.review
                .results
                .retain(|_, stored| stored.saved.is_some());
            self.summaries
                .retain(|id, _| self.review.results.contains_key(id));
            self.graph_analysis.clear();
            if let Err(error) = self.publish_document() {
                self.match_error(error);
            }
        }
        if !self.match_active() {
            self.update_focus().await;
        }
    }
}
async fn read_opening(path: &std::path::Path) -> Result<GameDocument, MatchError> {
    let result = async {
        let bytes = tokio::fs::read(path).await.map_err(|e| e.to_string())?;
        let collection = sgf::parse(&bytes).map_err(|e| e.to_string())?;
        let tree = collection
            .games
            .first()
            .ok_or_else(|| "opening contains no game".to_owned())?;
        let mut document = GameDocument::from_sgf(tree).map_err(|e| e.to_string())?;
        document
            .apply(document::Command::Step(i32::MAX))
            .map_err(|e| e.to_string())?;
        document
            .position_at(document.current())
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(document)
    }
    .await;
    result.map_err(|reason| MatchError::Opening {
        path: path.to_string_lossy().into_owned(),
        reason,
    })
}
