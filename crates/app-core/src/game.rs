//! Persisted play settings and snapshots. The application actor owns the session.
mod clock;
mod gtp_time;
mod result;
pub(crate) use gtp_time::GtpClock;
pub use gtp_time::GtpTime;

pub(crate) use clock::ClockState;
pub(crate) use result::{game_file_name, score_result};

use crate::{ColorView, NewGameOptions};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum Player {
    Human,
    Ai(AiPlayer),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum AiPlayer {
    KataGo(KataGoPlay),
    Gtp { engine_id: String, time: GtpTime },
}

impl Default for AiPlayer {
    fn default() -> Self {
        Self::KataGo(KataGoPlay::default())
    }
}

impl AiPlayer {
    pub fn validate(&self) -> Result<(), GameError> {
        match self {
            Self::KataGo(play) => play.validate(),
            Self::Gtp { engine_id, time } => {
                if engine_id.trim().is_empty() {
                    return Err(GameError::InvalidSetup("GTP engine ID is empty".into()));
                }
                time.validate()
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum GameStart {
    NewBoard(NewGameOptions),
    CurrentPosition,
    FreeHandicap { options: NewGameOptions, stones: u8 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct GameSetup {
    #[ts(as = "Option<Player>", optional)]
    pub black: Player,
    #[ts(as = "Option<Player>", optional)]
    pub white: Player,
    #[ts(as = "Option<GameStart>", optional)]
    pub start: GameStart,
    #[ts(optional = nullable)]
    pub human_clock: Option<HumanClock>,
    #[ts(as = "Option<bool>", optional)]
    pub ponder: bool,
    #[ts(as = "Option<bool>", optional)]
    pub auto_save: bool,
}

impl Default for GameSetup {
    fn default() -> Self {
        Self {
            black: Player::Human,
            white: Player::Ai(AiPlayer::default()),
            start: GameStart::NewBoard(NewGameOptions::default()),
            human_clock: None,
            ponder: true,
            auto_save: true,
        }
    }
}

impl GameSetup {
    pub fn validate(&self) -> Result<(), GameError> {
        self.validate_match()?;
        if matches!((&self.black, &self.white), (Player::Ai(_), Player::Ai(_))) {
            return Err(GameError::InvalidSetup(
                "use start_match for two AI players".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_match(&self) -> Result<(), GameError> {
        for player in [&self.black, &self.white] {
            if let Player::Ai(ai) = player {
                ai.validate()?;
            }
        }
        if let Some(clock) = &self.human_clock {
            clock.validate()?;
        }
        if let GameStart::NewBoard(options) | GameStart::FreeHandicap { options, .. } = &self.start
        {
            document::GameDocument::new_game(options.clone().into())
                .map_err(|error| GameError::InvalidSetup(error.to_string()))?;
        }
        if let GameStart::FreeHandicap { options, stones } = &self.start
            && (!(2..=9).contains(stones)
                || options.handicap != 0
                || ![&self.black, &self.white]
                    .iter()
                    .any(|p| matches!(p, Player::Ai(AiPlayer::Gtp { .. }))))
        {
            return Err(GameError::InvalidSetup(
                "free handicap requires a GTP player, 2..=9 stones and no fixed handicap".into(),
            ));
        }
        Ok(())
    }

    pub fn player(&self, color: go_core::Color) -> &Player {
        match color {
            go_core::Color::Black => &self.black,
            go_core::Color::White => &self.white,
        }
    }

    pub fn player_mut(&mut self, color: go_core::Color) -> &mut Player {
        match color {
            go_core::Color::Black => &mut self.black,
            go_core::Color::White => &mut self.white,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct KataGoPlay {
    #[ts(optional = nullable)]
    pub seconds: Option<f64>,
    #[ts(type = "number | null", optional = nullable)]
    pub visits: Option<u64>,
    #[ts(as = "Option<bool>", optional)]
    pub pure_network: bool,
    #[ts(optional = nullable)]
    pub resign: Option<ResignRule>,
    #[ts(optional = nullable)]
    pub random_opening: Option<RandomOpening>,
}

impl Default for KataGoPlay {
    fn default() -> Self {
        Self {
            seconds: Some(2.0),
            visits: None,
            pure_network: false,
            resign: Some(ResignRule::default()),
            random_opening: None,
        }
    }
}

impl KataGoPlay {
    pub fn validate(&self) -> Result<(), GameError> {
        if !self.pure_network && self.seconds.is_none() && self.visits.is_none() {
            return Err(GameError::InvalidSetup(
                "AI needs a time or visits limit".into(),
            ));
        }
        if self
            .seconds
            .is_some_and(|seconds| !seconds.is_finite() || seconds <= 0.0 || seconds > 86400.0)
            || self
                .visits
                .is_some_and(|visits| visits == 0 || visits > 9_007_199_254_740_991)
        {
            return Err(GameError::InvalidSetup(
                "invalid AI time or visits limit".into(),
            ));
        }
        if let Some(rule) = &self.resign
            && (rule.consecutive == 0
                || !rule.below_winrate.is_finite()
                || !(0.0..=1.0).contains(&rule.below_winrate))
        {
            return Err(GameError::InvalidSetup("invalid resignation rule".into()));
        }
        if let Some(opening) = &self.random_opening
            && (opening.within_moves == 0
                || !opening.max_winrate_drop.is_finite()
                || !(0.0..=1.0).contains(&opening.max_winrate_drop)
                || !opening.min_visit_share.is_finite()
                || !(0.0..=1.0).contains(&opening.min_visit_share))
        {
            return Err(GameError::InvalidSetup(
                "invalid random opening rule".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct ResignRule {
    #[ts(as = "Option<u32>", optional)]
    pub after_moves: u32,
    #[ts(as = "Option<u32>", optional)]
    pub consecutive: u32,
    #[ts(as = "Option<f64>", optional)]
    pub below_winrate: f64,
}
impl Default for ResignRule {
    fn default() -> Self {
        Self {
            after_moves: 150,
            consecutive: 2,
            below_winrate: 0.05,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct RandomOpening {
    #[ts(as = "Option<u32>", optional)]
    pub within_moves: u32,
    #[ts(as = "Option<f64>", optional)]
    pub max_winrate_drop: f64,
    #[ts(as = "Option<f64>", optional)]
    pub min_visit_share: f64,
}
impl Default for RandomOpening {
    fn default() -> Self {
        Self {
            within_moves: 20,
            max_winrate_drop: 0.005,
            min_visit_share: 0.05,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct HumanClock {
    #[ts(as = "Option<u32>", optional)]
    pub main_seconds: u32,
    #[ts(as = "Option<u32>", optional)]
    pub byoyomi_seconds: u32,
    #[ts(as = "Option<u32>", optional)]
    pub periods: u32,
}
impl Default for HumanClock {
    fn default() -> Self {
        Self {
            main_seconds: 1200,
            byoyomi_seconds: 30,
            periods: 3,
        }
    }
}
impl HumanClock {
    pub fn validate(&self) -> Result<(), GameError> {
        if self.main_seconds == 0 && (self.byoyomi_seconds == 0 || self.periods == 0) {
            return Err(GameError::InvalidSetup(
                "human clock needs main time or byoyomi".into(),
            ));
        }
        if (self.byoyomi_seconds == 0) != (self.periods == 0) {
            return Err(GameError::InvalidSetup(
                "byoyomi seconds and periods must both be enabled or disabled".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct HumanClockView {
    #[ts(as = "Option<f64>", optional)]
    pub main_seconds: f64,
    #[ts(as = "Option<u32>", optional)]
    pub periods: u32,
    #[ts(as = "Option<f64>", optional)]
    pub byoyomi_seconds: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum GamePhase {
    Playing,
    Paused,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum GameEndReason {
    Resignation,
    Timeout,
    Scored,
    Unscored,
    Stopped,
    EngineError,
    MoveLimit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct GameView {
    #[ts(as = "Option<GamePhase>", optional)]
    pub phase: GamePhase,
    #[ts(as = "Option<Player>", optional)]
    pub black: Player,
    #[ts(as = "Option<Player>", optional)]
    pub white: Player,
    #[ts(as = "Option<ColorView>", optional)]
    pub to_play: ColorView,
    #[ts(as = "Option<bool>", optional)]
    pub ai_thinking: bool,
    #[ts(optional = nullable)]
    pub black_clock: Option<HumanClockView>,
    #[ts(optional = nullable)]
    pub white_clock: Option<HumanClockView>,
    #[ts(as = "Option<u32>", optional)]
    pub move_number: u32,
    #[ts(as = "Option<u32>", optional)]
    pub start_node: u32,
    #[ts(as = "Option<u32>", optional)]
    pub latest_node: u32,
    #[ts(optional = nullable)]
    pub result: Option<String>,
    #[ts(optional = nullable)]
    pub end_reason: Option<GameEndReason>,
    #[ts(optional = nullable)]
    pub error: Option<GameError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PvPick {
    /// Zero-based candidate index after sorting by KataGo order.
    pub candidate: usize,
    pub moves: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "type", content = "data")]
pub enum GameError {
    #[error("invalid game setup: {0}")]
    InvalidSetup(String),
    #[error("a game is already active")]
    AlreadyActive,
    #[error("there is no active game")]
    NotActive,
    #[error("the game is not paused")]
    NotPaused,
    #[error("it is the AI's turn")]
    AiTurn,
    #[error("play is only allowed at the latest game node")]
    NotLatestNode,
    #[error("there is no human player to resign")]
    NoHumanPlayer,
    #[error("there is no human move to take back after the game start")]
    CannotTakeBack,
    #[error("the paused game position has changed")]
    PositionChanged,
    #[error("no analysis is available for this position")]
    NoAnalysis,
    #[error("the selected variation is unavailable or invalid")]
    InvalidPv,
    #[error("a configured engine is required")]
    NoEngine,
    #[error("the current mode is incompatible with a game")]
    IncompatibleMode,
    #[error("game engine failed: {0}")]
    Engine(String),
    #[error("GTP engine {engine_id} failed: {reason}")]
    Gtp { engine_id: String, reason: String },
    #[error("GTP engine does not support this time control")]
    GtpUnsupportedTime,
    #[error("GTP engine returned an illegal move: {0}")]
    GtpIllegalMove(String),
    #[error("the GTP player does not support play now")]
    GtpPlayNowUnsupported,
    #[error("未数子")]
    Unscored,
    #[error("game ended, but saving to {path} failed: {reason}")]
    AutoSave { path: String, reason: String },
}

#[cfg(test)]
mod tests;
