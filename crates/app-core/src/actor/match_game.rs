//! A match schedules successive games through the same actor-owned play session.
use super::{Actor, Command, Input};
use crate::{
    AppError, AppEvent, GameEndReason, GamePhase, GameSetup, GameStart, MatchError, MatchPhase,
    MatchPlayer, MatchSetup, MatchSide, MatchView, Player, match_game::Request,
};
use document::{GameDocument, NodeId};
use engine_session::EngineHandle;
use go_core::Color;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
mod control;
mod lifecycle;

pub(super) struct Session {
    setup: MatchSetup,
    view: MatchView,
    engines: BTreeMap<String, EngineHandle>,
    owned_engines: BTreeSet<String>,
    gtp_engines: BTreeSet<String>,
    names: [String; 2],
    current_seed: Option<GameDocument>,
    directory: Option<PathBuf>,
    random: u64,
    pending_game: bool,
    move_limit: u32,
    end_reason: Option<String>,
}
impl Session {
    fn side_for_color(&self, color: Color) -> MatchSide {
        if (color == Color::Black) == self.view.a_is_black {
            MatchSide::A
        } else {
            MatchSide::B
        }
    }
    fn player(&self, side: MatchSide) -> &MatchPlayer {
        match side {
            MatchSide::A => &self.setup.a,
            MatchSide::B => &self.setup.b,
        }
    }
    fn names(&self) -> (&str, &str) {
        if self.view.a_is_black {
            (&self.names[0], &self.names[1])
        } else {
            (&self.names[1], &self.names[0])
        }
    }
}
impl Actor {
    pub(super) fn match_active(&self) -> bool {
        self.match_session
            .as_ref()
            .is_some_and(|s| matches!(s.view.phase, MatchPhase::Running | MatchPhase::Paused))
    }
    pub(super) fn match_view(&self) -> Option<MatchView> {
        self.match_session.as_ref().map(|s| s.view.clone())
    }
    fn publish_match(&self) {
        if let Some(view) = self.match_view() {
            self.output.send(AppEvent::MatchUpdated(view));
        }
    }
    pub(super) fn match_gtp_engine_busy(&self, id: &str) -> bool {
        self.match_active()
            && self
                .match_session
                .as_ref()
                .is_some_and(|s| s.gtp_engines.contains(id))
    }
    pub(super) fn match_needs_advance(&self) -> bool {
        self.match_session.as_ref().is_some_and(|s| {
            s.view.phase == MatchPhase::Running
                && (s.pending_game
                    || self.game_view().is_some_and(|g| {
                        g.phase == GamePhase::Finished
                            || (g.phase == GamePhase::Paused && g.error.is_some())
                    }))
        })
    }
    pub(super) fn match_engine_for_turn(&self) -> Option<EngineHandle> {
        let session = self
            .match_session
            .as_ref()
            .filter(|_| self.match_active())?;
        let color = self.game_view().map_or(Color::Black, |v| v.to_play.into());
        match session.player(session.side_for_color(color)) {
            MatchPlayer::KataGo { engine_id, .. } => session.engines.get(engine_id).cloned(),
            _ => session.engines.values().next().cloned(),
        }
    }
    pub(super) fn match_override_settings(&self, request: &mut engine_session::AnalysisRequest) {
        let Some(session) = self.match_session.as_ref().filter(|_| self.match_active()) else {
            return;
        };
        let color = self.game_view().map_or(Color::Black, |v| v.to_play.into());
        if let MatchPlayer::KataGo { wrn, pda, .. } = session.player(session.side_for_color(color))
        {
            let settings = request
                .override_settings
                .get_or_insert_with(Default::default);
            settings.insert("wideRootNoise".into(), serde_json::json!(wrn));
            settings.insert("playoutDoublingAdvantage".into(), serde_json::json!(pda));
        }
    }
    pub(super) async fn match_move_limit_reached(&mut self) -> Result<bool, AppError> {
        let Some(session) = self.match_session.as_ref().filter(|_| self.match_active()) else {
            return Ok(false);
        };
        let Some(game) = self.game_view() else {
            return Ok(false);
        };
        if game.move_number < session.move_limit {
            return Ok(false);
        }
        let position = self.document.position_at(NodeId(game.latest_node))?;
        if position.moves.len() >= 2
            && position
                .moves
                .iter()
                .rev()
                .take(2)
                .all(|mv| matches!(mv, go_core::Move::Pass(_)))
        {
            return Ok(false);
        }
        if let Some(session) = &mut self.match_session {
            session.end_reason = Some("超手数".into());
        }
        self.finish_game(None, GameEndReason::MoveLimit).await?;
        Ok(true)
    }
    pub(super) fn match_guard(&self, command: Command) -> Option<Command> {
        if !self.match_active() {
            return Some(command);
        }
        macro_rules! reject {
            ($reply:expr) => {{
                let _ = $reply.send(Err(MatchError::AlreadyActive.into()));
                None
            }};
        }
        match command {
            Command::Game(_, reply) => reject!(reply),
            Command::SetAiPlayer(_, _, reply) => reject!(reply),
            Command::PlayBestMove(_, reply) => reject!(reply),
            Command::Review(_, reply) => reject!(reply),
            Command::Batch(_, reply) => reject!(reply),
            Command::SetEngineSettings(_, reply) => reject!(reply),
            Command::Remote(
                crate::remote::Request::Add(_, _, reply)
                | crate::remote::Request::Update(_, _, _, reply)
                | crate::remote::Request::Local(reply),
            ) => {
                reject!(reply)
            }
            Command::ReplaceMainEngine(_, _, _, reply) | Command::Engines(_, reply) => {
                reject!(reply)
            }
            Command::TryConfigureEngine(_, _, reply) => reject!(reply),
            Command::BeginAutoSetup(reply) => reject!(reply),
            Command::TryAutoEngine(_, reply) => reject!(reply),
            Command::JavaImport(crate::java_import::Request::Confirm(_, reply)) => reject!(reply),
            Command::BoardSync(crate::board_sync::Request::Start(_, _), reply) => reject!(reply),
            Command::Benchmark(
                crate::benchmark::Request::Start | crate::benchmark::Request::Reset,
                reply,
            ) => reject!(reply),
            Command::AnalysisLimit(crate::analysis_limit::Request::Toggle(reply)) => reject!(reply),
            Command::Apply(input, reply)
                if matches!(
                    input,
                    Input::Edit(document::Command::Step(_) | document::Command::GoTo(_))
                        | Input::Shutdown
                ) =>
            {
                Some(Command::Apply(input, reply))
            }
            Command::Apply(input, reply) if blocked(&input) => reject!(reply),
            other => Some(other),
        }
    }
}
fn blocked(input: &Input) -> bool {
    matches!(
        input,
        Input::Edit(_)
            | Input::OpenBytes(_)
            | Input::OpenPath(_)
            | Input::OpenDroppedPaths(_)
            | Input::NewGame(_)
            | Input::PasteSgf
            | Input::ConvertToStartPosition
            | Input::StartTrial
            | Input::EndTrial
            | Input::GenerateTsumego(_)
            | Input::ExitTsumego
            | Input::Undo
            | Input::Redo
            | Input::ClearAnalysis(_)
            | Input::ConfigureEngine(_)
            | Input::SetPureNetwork(_)
            | Input::SetPondering(_)
            | Input::EnsureNoBatch
            | Input::PrepareUpdateInstall
    )
}
