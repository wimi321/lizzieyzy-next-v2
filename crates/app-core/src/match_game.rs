//! Match configuration, standings and commands for the existing play session.
mod stats;
#[cfg(test)]
mod tests;
use crate::{
    AiPlayer, AppCore, AppError, GameStart, GtpTime, KataGoPlay, RandomOpening, ResignRule,
    actor::Command,
};
use serde::{Deserialize, Serialize};
pub(crate) use stats::{file_component, match_file_name};
use std::{collections::BTreeMap, path::PathBuf};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum MatchSide {
    A,
    B,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum MatchPlayer {
    KataGo {
        engine_id: String,
        #[serde(default = "match_play")]
        #[ts(as = "Option<KataGoPlay>", optional)]
        play: KataGoPlay,
        #[serde(default)]
        #[ts(as = "Option<f64>", optional)]
        wrn: f64,
        #[serde(default)]
        #[ts(as = "Option<f64>", optional)]
        pda: f64,
    },
    Gtp {
        engine_id: String,
        time: GtpTime,
    },
}
fn match_play() -> KataGoPlay {
    KataGoPlay {
        resign: Some(ResignRule {
            after_moves: 0,
            consecutive: 2,
            below_winrate: 0.1,
        }),
        ..Default::default()
    }
}
impl Default for MatchPlayer {
    fn default() -> Self {
        Self::KataGo {
            engine_id: String::new(),
            play: match_play(),
            wrn: 0.0,
            pda: 0.0,
        }
    }
}
impl MatchPlayer {
    pub fn engine_id(&self) -> &str {
        match self {
            Self::KataGo { engine_id, .. } | Self::Gtp { engine_id, .. } => engine_id,
        }
    }
    pub(crate) fn player(&self, random: Option<RandomOpening>) -> AiPlayer {
        match self {
            Self::KataGo { play, .. } => {
                let mut play = play.clone();
                if random.is_some() {
                    play.random_opening = random;
                }
                AiPlayer::KataGo(play)
            }
            Self::Gtp { engine_id, time } => AiPlayer::Gtp {
                engine_id: engine_id.clone(),
                time: time.clone(),
            },
        }
    }
    fn validate(&self) -> Result<(), MatchError> {
        if self.engine_id().trim().is_empty() {
            return Err(MatchError::InvalidSetup(
                "an engine entry is required".into(),
            ));
        }
        self.player(None)
            .validate()
            .map_err(|error| MatchError::InvalidSetup(error.to_string()))?;
        if let Self::KataGo { wrn, pda, .. } = self {
            validate_params(*wrn, *pda)?;
        }
        Ok(())
    }
}
pub(crate) fn validate_params(wrn: f64, pda: f64) -> Result<(), MatchError> {
    if !wrn.is_finite()
        || !(0.0..=5.0).contains(&wrn)
        || !pda.is_finite()
        || !(-3.0..=3.0).contains(&pda)
    {
        Err(MatchError::InvalidSetup(
            "WRN must be 0..=5 and PDA must be -3..=3".into(),
        ))
    } else {
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct MatchSetup {
    #[ts(as = "Option<MatchPlayer>", optional)]
    pub a: MatchPlayer,
    #[ts(as = "Option<MatchPlayer>", optional)]
    pub b: MatchPlayer,
    #[ts(as = "Option<u32>", optional)]
    pub games: u32,
    #[ts(as = "Option<bool>", optional)]
    pub swap_colors: bool,
    #[ts(as = "Option<GameStart>", optional)]
    pub start: GameStart,
    #[ts(optional = nullable)]
    pub max_moves: Option<u32>,
    #[ts(as = "Option<Vec<String>>", optional)]
    pub openings: Vec<PathBuf>,
    #[ts(as = "Option<bool>", optional)]
    pub random_openings: bool,
    #[ts(optional = nullable)]
    pub random_opening: Option<RandomOpening>,
    #[ts(as = "Option<bool>", optional)]
    pub auto_save: bool,
}
impl Default for MatchSetup {
    fn default() -> Self {
        Self {
            a: Default::default(),
            b: Default::default(),
            games: 1,
            swap_colors: true,
            start: GameStart::NewBoard(Default::default()),
            max_moves: None,
            openings: Vec::new(),
            random_openings: false,
            random_opening: None,
            auto_save: true,
        }
    }
}
impl MatchSetup {
    pub fn validate(&self) -> Result<(), MatchError> {
        self.a.validate()?;
        self.b.validate()?;
        crate::GameSetup {
            black: crate::Player::Ai(self.a.player(self.random_opening)),
            white: crate::Player::Ai(self.b.player(self.random_opening)),
            start: self.start.clone(),
            auto_save: false,
            ..Default::default()
        }
        .validate_match()
        .map_err(|error| MatchError::InvalidSetup(error.to_string()))?;
        if self.games == 0
            || self.games > 1_000_000
            || self.max_moves == Some(0)
            || self.openings.len() > 10_000
        {
            return Err(MatchError::InvalidSetup(
                "invalid game count, move limit or opening count".into(),
            ));
        }
        if matches!(self.start, GameStart::CurrentPosition) && !self.openings.is_empty() {
            return Err(MatchError::InvalidSetup(
                "current position and opening files are mutually exclusive".into(),
            ));
        }
        if let GameStart::NewBoard(options) = &self.start {
            document::GameDocument::new_game(options.clone().into())
                .map_err(|error| MatchError::InvalidSetup(error.to_string()))?;
        }
        for player in [&self.a, &self.b] {
            player
                .player(self.random_opening)
                .validate()
                .map_err(|error| MatchError::InvalidSetup(error.to_string()))?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum MatchPhase {
    Running,
    Paused,
    Completed,
    Stopped,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MatchWins {
    #[ts(as = "Option<u32>", optional)]
    pub total: u32,
    #[ts(as = "Option<u32>", optional)]
    pub black: u32,
    #[ts(as = "Option<u32>", optional)]
    pub white: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct EloEstimate {
    #[ts(as = "Option<f64>", optional)]
    pub elo: f64,
    #[ts(optional = nullable)]
    pub lower: Option<f64>,
    #[ts(optional = nullable)]
    pub upper: Option<f64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OpeningStanding {
    #[ts(as = "Option<String>", optional)]
    pub opening: String,
    #[ts(as = "Option<u32>", optional)]
    pub completed: u32,
    #[ts(as = "Option<MatchWins>", optional)]
    pub a: MatchWins,
    #[ts(as = "Option<MatchWins>", optional)]
    pub b: MatchWins,
    #[ts(as = "Option<u32>", optional)]
    pub draws: u32,
    #[ts(as = "Option<BTreeMap<String,u32>>", optional)]
    pub unscored: BTreeMap<String, u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct MatchView {
    #[ts(as = "Option<MatchPhase>", optional)]
    pub phase: MatchPhase,
    #[ts(as = "Option<u32>", optional)]
    pub games: u32,
    #[ts(as = "Option<u32>", optional)]
    pub completed: u32,
    #[ts(as = "Option<u32>", optional)]
    pub current_game: u32,
    #[ts(as = "Option<bool>", optional)]
    pub a_is_black: bool,
    #[ts(as = "Option<MatchWins>", optional)]
    pub a: MatchWins,
    #[ts(as = "Option<MatchWins>", optional)]
    pub b: MatchWins,
    #[ts(as = "Option<u32>", optional)]
    pub draws: u32,
    #[ts(as = "Option<BTreeMap<String,u32>>", optional)]
    pub unscored: BTreeMap<String, u32>,
    #[ts(as = "Option<Vec<OpeningStanding>>", optional)]
    pub openings: Vec<OpeningStanding>,
    #[ts(optional = nullable)]
    pub current_opening: Option<String>,
    #[ts(optional = nullable)]
    pub elo: Option<EloEstimate>,
    #[ts(optional = nullable)]
    pub error: Option<MatchError>,
    #[ts(optional = nullable)]
    pub save_directory: Option<String>,
}
impl MatchView {
    pub(crate) fn new(games: u32) -> Self {
        Self {
            phase: MatchPhase::Running,
            games,
            completed: 0,
            current_game: 0,
            a_is_black: true,
            a: Default::default(),
            b: Default::default(),
            draws: 0,
            unscored: BTreeMap::new(),
            openings: Vec::new(),
            current_opening: None,
            elo: None,
            error: None,
            save_directory: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "type", content = "data")]
pub enum MatchError {
    #[error("invalid match setup: {0}")]
    InvalidSetup(String),
    #[error("a match is already active")]
    AlreadyActive,
    #[error("there is no active match")]
    NotActive,
    #[error("the match is not paused")]
    NotPaused,
    #[error("engine entry not found: {0}")]
    EngineMissing(String),
    #[error("this operation requires a KataGo player")]
    NotKataGo,
    #[error("opening {path} failed: {reason}")]
    Opening { path: String, reason: String },
    #[error("saving match game to {path} failed: {reason}")]
    Save { path: String, reason: String },
    #[error("match engine failed: {0}")]
    Engine(String),
}
pub(crate) enum Request {
    Start(Box<MatchSetup>),
    Pause,
    Resume,
    Stop,
    Games(u32),
    PlayNow,
    Resign(go_core::Color),
    Params(MatchSide, f64, f64),
}
impl AppCore {
    pub async fn start_match(&self, setup: MatchSetup) -> Result<MatchView, AppError> {
        self.match_request(Request::Start(Box::new(setup))).await
    }
    pub async fn pause_match(&self) -> Result<MatchView, AppError> {
        self.match_request(Request::Pause).await
    }
    pub async fn resume_match(&self) -> Result<MatchView, AppError> {
        self.match_request(Request::Resume).await
    }
    pub async fn stop_match(&self) -> Result<MatchView, AppError> {
        self.match_request(Request::Stop).await
    }
    pub async fn set_match_games(&self, games: u32) -> Result<MatchView, AppError> {
        self.match_request(Request::Games(games)).await
    }
    pub async fn match_play_now(&self) -> Result<MatchView, AppError> {
        self.match_request(Request::PlayNow).await
    }
    pub async fn match_resign(&self, color: go_core::Color) -> Result<MatchView, AppError> {
        self.match_request(Request::Resign(color)).await
    }
    pub async fn set_match_side_params(
        &self,
        side: MatchSide,
        wrn: f64,
        pda: f64,
    ) -> Result<MatchView, AppError> {
        self.match_request(Request::Params(side, wrn, pda)).await
    }
    async fn match_request(&self, request: Request) -> Result<MatchView, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Match(request, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
    pub async fn match_view(&self) -> Result<Option<MatchView>, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::MatchView(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }
}
