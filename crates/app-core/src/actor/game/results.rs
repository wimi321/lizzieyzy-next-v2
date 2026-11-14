use super::*;
use crate::KataGoPlay;
use katago_protocol::AnalysisResponse;

pub(super) fn win(winner: Color, suffix: &str) -> String {
    format!(
        "{}+{suffix}",
        if winner == Color::Black { "B" } else { "W" }
    )
}
impl Actor {
    pub(super) fn ai_resigns(&mut self, play: &KataGoPlay, response: &AnalysisResponse) -> bool {
        let Some(game) = &mut self.game else {
            return false;
        };
        let color = game.color();
        let winrate = if color == Color::Black {
            response.root_info.winrate
        } else {
            1.0 - response.root_info.winrate
        };
        let Some(rule) = &play.resign else {
            game.low_winrates[index(color)] = 0;
            return false;
        };
        if game.view.move_number >= rule.after_moves
            && winrate.is_finite()
            && winrate < rule.below_winrate
        {
            game.low_winrates[index(color)] = game.low_winrates[index(color)].saturating_add(1);
        } else {
            game.low_winrates[index(color)] = 0;
        }
        game.low_winrates[index(color)] >= rule.consecutive
    }
    pub(super) fn select_game_move(
        &mut self,
        play: &KataGoPlay,
        response: &AnalysisResponse,
    ) -> Result<Move, AppError> {
        let game = self.require_game()?;
        let position = self.document.position_at(NodeId(game.view.latest_node))?;
        let color = game.color();
        let mut candidates: Vec<_> = response.move_infos.iter().collect();
        candidates.sort_by_key(|candidate| candidate.order);
        let best = *candidates.first().ok_or(GameError::NoAnalysis)?;
        // A pass recommendation is final; random opening never changes it.
        let choice = if !best.coordinate.eq_ignore_ascii_case("pass") {
            if let Some(opening) = &play.random_opening
                && game.view.move_number < opening.within_moves
            {
                let rate = |value: f64| {
                    if color == Color::Black {
                        value
                    } else {
                        1.0 - value
                    }
                };
                let best_rate = rate(best.winrate);
                let eligible: Vec<_> = candidates
                    .into_iter()
                    .filter(|candidate| {
                        let winrate = rate(candidate.winrate);
                        winrate.is_finite()
                            && winrate >= best_rate - opening.max_winrate_drop
                            && candidate.visits as f64
                                >= best.visits as f64 * opening.min_visit_share
                            && Move::from_gtp(color, &candidate.coordinate, position.board.size())
                                .is_ok_and(|mv| {
                                    let mut board = position.board.clone();
                                    board.play(mv).is_ok()
                                })
                    })
                    .collect();
                if eligible.is_empty() {
                    return Err(GameError::NoAnalysis.into());
                }
                let game = self.game.as_mut().ok_or(GameError::NotActive)?;
                // SplitMix64: an actor-owned, seedable non-cryptographic PRNG.
                game.random = game.random.wrapping_add(0x9e3779b97f4a7c15);
                let mut value = game.random;
                value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
                value ^= value >> 31;
                eligible[(value % eligible.len() as u64) as usize]
            } else {
                best
            }
        } else {
            best
        };
        let mv = Move::from_gtp(color, &choice.coordinate, position.board.size())
            .map_err(|_| GameError::InvalidPv)?;
        let mut board = position.board;
        board.play(mv).map_err(|_| GameError::InvalidPv)?;
        Ok(mv)
    }
    pub(super) fn set_game_result(&mut self, result: Option<String>) -> Result<(), AppError> {
        let info = self.document.game_info()?;
        let before = self.document.content_revision();
        self.document
            .apply(document::Command::SetGameInfo(document::GameInfoEdit {
                black_name: info.black_name,
                black_rank: info.black_rank,
                white_name: info.white_name,
                white_rank: info.white_rank,
                komi: info.komi,
                rules: info.rules_preset,
                result,
                date: info.date,
                event: info.event,
            }))?;
        if self.document.content_revision() != before {
            self.file_state.analysis_edits.document_edit();
        }
        Ok(())
    }
    pub(super) async fn finish_game(
        &mut self,
        result: Option<String>,
        reason: GameEndReason,
    ) -> Result<(), AppError> {
        self.require_game()?;
        self.cancel_game_query().await?;
        self.clear_game_live_query().await?;
        let valid = self.validate_game_position().is_ok();
        if let Some(game) = &mut self.game {
            for clock in game.clocks.iter_mut().flatten() {
                clock.pause(Instant::now());
            }
            game.view.phase = GamePhase::Finished;
            game.view.result = result.clone();
            game.view.end_reason = Some(reason);
        }
        // A paused session may have been replaced by a different document.
        if !valid {
            self.publish_game();
            return Err(GameError::PositionChanged.into());
        }
        self.set_game_result(result)?;
        self.document_changed().await?;
        self.publish_game();
        if self.game.as_ref().is_some_and(|game| game.setup.auto_save)
            && let Err(error) = self.auto_save_game().await
        {
            if let Some(game) = &mut self.game {
                game.view.error = Some(error.clone());
            }
            self.publish_game();
            return Err(error.into());
        }
        Ok(())
    }
    async fn auto_save_game(&mut self) -> Result<(), GameError> {
        let directory = self
            .settings
            .games_directory
            .clone()
            .unwrap_or_else(|| self.directory.join("MyGames"));
        let result = async {
            tokio::fs::create_dir_all(&directory).await?;
            let info = self.document.game_info().map_err(std::io::Error::other)?;
            let name = crate::game::game_file_name(
                &self.config.batch_clock.local_timestamp(),
                info.black_name.as_deref().unwrap_or("Black"),
                info.white_name.as_deref().unwrap_or("White"),
            )
            .ok_or_else(|| std::io::Error::other("invalid game timestamp"))?;
            let (tree, warnings) = self
                .export_document_with_analysis()
                .map_err(std::io::Error::other)?;
            let bytes = sgf::serialize(&sgf::Collection {
                games: vec![tree],
                encoding: "UTF-8".into(),
                warnings: Vec::new(),
            })
            .into_bytes();
            let mut number = 1_u32;
            loop {
                let name = if number == 1 {
                    name.clone()
                } else {
                    format!("{}-{number}.sgf", name.trim_end_matches(".sgf"))
                };
                let path = std::path::absolute(directory.join(name))?;
                match crate::storage::write_new(path.clone(), bytes.clone()).await {
                    Ok(()) => {
                        self.file_name = path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned());
                        self.file_state.mark_saved(&self.document, path);
                        self.file_state.save_warnings = warnings;
                        self.publish_document().map_err(std::io::Error::other)?;
                        return Ok::<_, std::io::Error>(());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        number = number
                            .checked_add(1)
                            .ok_or_else(|| std::io::Error::other("output collision limit"))?;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        .await;
        result.map_err(|error| GameError::AutoSave {
            path: directory.to_string_lossy().into_owned(),
            reason: error.to_string(),
        })
    }
}
