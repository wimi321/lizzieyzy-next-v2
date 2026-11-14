use super::*;
use crate::{AiPlayer, GtpTime, NewGameOptions};
use document::GamePosition;
use engine_session::gtp::{GtpHandle, GtpInfo};
use std::collections::BTreeMap;

pub(super) fn error(id: &str, error: impl std::fmt::Display) -> GameError {
    GameError::Gtp {
        engine_id: id.to_owned(),
        reason: error.to_string(),
    }
}

pub(super) async fn send(
    handle: &GtpHandle,
    id: &str,
    text: impl Into<String>,
) -> Result<String, GameError> {
    handle
        .command(text)
        .await
        .map(|response| response.body)
        .map_err(|e| error(id, e))
}

pub(super) async fn board_setup(
    handle: &GtpHandle,
    info: &GtpInfo,
    id: &str,
    position: &GamePosition,
) -> Result<(), GameError> {
    let size = position.board.size();
    if size.width != size.height {
        return Err(GameError::InvalidSetup(
            "standard GTP requires a square board".into(),
        ));
    }
    send(handle, id, format!("boardsize {}", size.width)).await?;
    send(handle, id, "clear_board").await?;
    send(handle, id, format!("komi {}", position.komi)).await?;
    if info.supports("kata-set-rules") {
        send(
            handle,
            id,
            format!("kata-set-rules {}", position.rules.as_katago_str()),
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn full_sync(
    state: &mut gtp::State,
    position: &GamePosition,
) -> Result<(), GameError> {
    board_setup(&state.handle, &state.info, &state.id, position).await?;
    if position.handicap >= 2
        && !position.initial_stones.is_empty()
        && position
            .initial_stones
            .iter()
            .all(|(c, _)| *c == Color::Black)
    {
        let coordinates = position
            .initial_stones
            .iter()
            .map(|(_, p)| {
                p.to_gtp(position.board.size())
                    .map_err(|e| error(&state.id, e))
            })
            .collect::<Result<Vec<_>, _>>()?;
        send(
            &state.handle,
            &state.id,
            format!("set_free_handicap {}", coordinates.join(" ")),
        )
        .await?;
    } else {
        for (color, point) in &position.initial_stones {
            play(state, Move::Play(*color, *point), position.board.size()).await?;
        }
    }
    for mv in &position.analysis_moves {
        play(state, *mv, position.board.size()).await?;
    }
    state.moves = position.analysis_moves.clone();
    state.initialized = true;
    Ok(())
}

pub(super) async fn play(
    state: &gtp::State,
    mv: Move,
    size: go_core::BoardSize,
) -> Result<(), GameError> {
    let color = if mv.color() == Color::Black {
        "black"
    } else {
        "white"
    };
    let coordinate = mv.to_gtp(size).map_err(|e| error(&state.id, e))?;
    send(
        &state.handle,
        &state.id,
        format!("play {color} {coordinate}"),
    )
    .await?;
    Ok(())
}

pub(super) async fn sync(state: &mut gtp::State, position: &GamePosition) -> Result<(), GameError> {
    if !state.initialized || !position.analysis_moves.starts_with(&state.moves) {
        return full_sync(state, position).await;
    }
    for mv in &position.analysis_moves[state.moves.len()..] {
        play(state, *mv, position.board.size()).await?;
    }
    state.moves = position.analysis_moves.clone();
    Ok(())
}

pub(super) async fn set_time(
    state: &mut gtp::State,
    color: Color,
    time: &GtpTime,
) -> Result<(), GameError> {
    set_clock(state, color, time);
    if state.last_time.as_ref() != Some(time) {
        for text in time.commands(state.info.supports("kata-time_settings"))? {
            send(&state.handle, &state.id, text).await?;
        }
        state.last_time = Some(time.clone());
    }
    Ok(())
}

pub(super) fn set_clock(state: &mut gtp::State, color: Color, time: &GtpTime) {
    let slot = &mut state.clocks[index(color)];
    if slot.as_ref().is_none_or(|c| c.mode != *time) {
        *slot = Some(crate::game::GtpClock::new(time.clone()));
    }
}

impl Actor {
    pub(super) async fn prepare_gtp_players(
        &mut self,
        setup: &GameSetup,
        position: &GamePosition,
    ) -> Result<BTreeMap<String, gtp::State>, AppError> {
        let mut states = BTreeMap::new();
        for color in [Color::Black, Color::White] {
            if let Player::Ai(AiPlayer::Gtp { engine_id, time }) = setup.player(color) {
                if !states.contains_key(engine_id) {
                    let handle = self.get_gtp_handle(engine_id).await?;
                    let info = handle.ready().await.map_err(|e| error(engine_id, e))?;
                    let mut state = gtp::State::new(engine_id.clone(), handle, info);
                    full_sync(&mut state, position).await?;
                    states.insert(engine_id.clone(), state);
                }
                if let Some(state) = states.get_mut(engine_id) {
                    set_time(state, color, time).await?;
                }
            }
        }
        Ok(states)
    }

    pub(super) async fn gtp_free_handicap(
        &mut self,
        setup: &GameSetup,
        options: &NewGameOptions,
        stones: u8,
    ) -> Result<GameDocument, AppError> {
        let engine_id = [&setup.black, &setup.white]
            .iter()
            .find_map(|p| match p {
                Player::Ai(AiPlayer::Gtp { engine_id, .. }) => Some(engine_id),
                _ => None,
            })
            .ok_or_else(|| GameError::InvalidSetup("free handicap needs a GTP player".into()))?;
        let mut doc = GameDocument::new_game(options.clone().into())?;
        let position = doc.position_at(doc.current())?;
        let handle = self.get_gtp_handle(engine_id).await?;
        let info = handle.ready().await.map_err(|e| error(engine_id, e))?;
        board_setup(&handle, &info, engine_id, &position).await?;
        let body = send(&handle, engine_id, format!("place_free_handicap {stones}")).await?;
        let points = body
            .split_whitespace()
            .map(|text| {
                go_core::Point::from_gtp(text, position.board.size())
                    .map_err(|e| error(engine_id, e))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if points.len() != stones as usize
            || points
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != points.len()
        {
            return Err(error(engine_id, "invalid free handicap points").into());
        }
        for point in points {
            doc.apply(document::Command::SetSetupStone {
                point,
                color: Some(Color::Black),
            })?;
        }
        doc.apply(document::Command::SetFirstPlayer(Color::White))?;
        let mut tree = doc.to_sgf();
        if let Some(root) = tree.sequence.first_mut() {
            root.properties.retain(|p| p.identifier != "HA");
            root.properties.push(sgf::Property {
                identifier: "HA".into(),
                values: vec![stones.to_string()],
            });
        }
        let doc = GameDocument::from_sgf(&tree)?;
        doc.position_at(doc.current())?;
        Ok(doc)
    }

    pub(super) async fn undo_gtp_players(&mut self) -> Result<(), AppError> {
        let game = self.require_game()?;
        let position = self.document.position_at(NodeId(game.view.latest_node))?;
        let ids: Vec<_> = game.gtp_players.keys().cloned().collect();
        for id in ids {
            let Some(mut state) = self
                .game
                .as_ref()
                .and_then(|g| g.gtp_players.get(&id).cloned())
            else {
                continue;
            };
            if state.initialized
                && state.moves.starts_with(&position.analysis_moves)
                && state.info.supports("undo")
            {
                let count = state.moves.len() - position.analysis_moves.len();
                for _ in 0..count {
                    if send(&state.handle, &id, "undo").await.is_err() {
                        state.initialized = false;
                        break;
                    }
                }
                if state.initialized {
                    state.moves = position.analysis_moves.clone();
                }
            } else {
                state.initialized = false;
            }
            if !state.initialized {
                if let engine_session::gtp::GtpStatus::Failed(reason) =
                    state.handle.status().borrow().clone()
                {
                    return Err(error(&id, reason).into());
                }
                if matches!(
                    *state.handle.status().borrow(),
                    engine_session::gtp::GtpStatus::Stopped
                ) {
                    state.last_time = None;
                }
                state.handle = self.get_gtp_handle(&id).await?;
                state.info = state.handle.ready().await.map_err(|e| error(&id, e))?;
                full_sync(&mut state, &position).await?;
            }
            if let Some(game) = &mut self.game {
                game.gtp_players.insert(id, state);
            }
        }
        Ok(())
    }
}
