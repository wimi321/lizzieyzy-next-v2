//! Commands for the single actor-owned play session.
use crate::{AiPlayer, AppCore, AppError, GameSetup, GameView, PvPick, actor::Command};
use go_core::Color;
use tokio::sync::oneshot;

pub(crate) enum Request {
    Start(Box<GameSetup>),
    Pause,
    Resume,
    Stop,
    Resign,
    TakeBack,
    Continue(Option<Color>),
}

impl AppCore {
    pub async fn start_game(&self, setup: GameSetup) -> Result<GameView, AppError> {
        self.game_request(Request::Start(Box::new(setup))).await
    }
    pub async fn pause_game(&self) -> Result<GameView, AppError> {
        self.game_request(Request::Pause).await
    }
    pub async fn resume_game(&self) -> Result<GameView, AppError> {
        self.game_request(Request::Resume).await
    }
    pub async fn stop_game(&self) -> Result<GameView, AppError> {
        self.game_request(Request::Stop).await
    }
    pub async fn resign_game(&self) -> Result<GameView, AppError> {
        self.game_request(Request::Resign).await
    }
    pub async fn take_back(&self) -> Result<GameView, AppError> {
        self.game_request(Request::TakeBack).await
    }
    pub async fn continue_game(&self, ai_color: Option<Color>) -> Result<GameView, AppError> {
        self.game_request(Request::Continue(ai_color)).await
    }
    async fn game_request(&self, request: Request) -> Result<GameView, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Game(request, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
    pub async fn game_view(&self) -> Result<Option<GameView>, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::GameView(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }
    pub async fn set_ai_player(
        &self,
        color: Color,
        player: AiPlayer,
    ) -> Result<Option<GameView>, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::SetAiPlayer(color, player, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
    pub async fn play_best_move(&self, pv: Option<PvPick>) -> Result<u64, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::PlayBestMove(pv, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}
