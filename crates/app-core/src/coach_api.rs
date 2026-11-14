//! Actor commands for the HumanSL player and its coaching session.
use crate::{AppCore, AppError, CoachDecision, CoachHint, CoachSetup, CoachView, actor::Command};
use tokio::sync::oneshot;

pub(crate) enum Request {
    Start(CoachSetup, oneshot::Sender<Result<CoachView, AppError>>),
    Hint(oneshot::Sender<Result<CoachHint, AppError>>),
    Decision(CoachDecision, oneshot::Sender<Result<CoachHint, AppError>>),
    Review(oneshot::Sender<Result<(), AppError>>),
    View(oneshot::Sender<Option<CoachView>>),
}
impl AppCore {
    pub async fn start_coach(&self, setup: CoachSetup) -> Result<CoachView, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Coach(Request::Start(setup, reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
    pub async fn coach_hint(&self) -> Result<CoachHint, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Coach(Request::Hint(reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
    pub async fn coach_decision(&self, decision: CoachDecision) -> Result<CoachHint, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Coach(Request::Decision(decision, reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
    pub async fn coach_review(&self) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Coach(Request::Review(reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
    pub async fn coach_view(&self) -> Result<Option<CoachView>, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Coach(Request::View(reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }
}
