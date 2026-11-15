//! Manual final scoring DTOs and actor commands; no frontend scoring logic.

use crate::{AppCore, AppError, ColorView, PointView, actor};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ScoringRuleView {
    Area,
    Territory,
}

impl From<ScoringRuleView> for go_core::ScoringRule {
    fn from(value: ScoringRuleView) -> Self {
        match value {
            ScoringRuleView::Area => Self::Area,
            ScoringRuleView::Territory => Self::Territory,
        }
    }
}
impl From<go_core::ScoringRule> for ScoringRuleView {
    fn from(value: go_core::ScoringRule) -> Self {
        match value {
            go_core::ScoringRule::Area => Self::Area,
            go_core::ScoringRule::Territory => Self::Territory,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct ScoringBreakdown {
    pub living_stones: usize,
    pub territory: usize,
    /// This side's dead stones; territory scoring awards these to its opponent.
    pub dead_stones: usize,
    /// Prisoners taken before scoring, excluding the newly marked dead stones.
    pub captures: usize,
    pub komi: f64,
    pub handicap_compensation: u32,
    pub total: f64,
}
impl From<go_core::scoring::ScoreBreakdown> for ScoringBreakdown {
    fn from(value: go_core::scoring::ScoreBreakdown) -> Self {
        Self {
            living_stones: value.living_stones,
            territory: value.territory,
            dead_stones: value.dead_stones,
            captures: value.captures,
            komi: value.komi,
            handicap_compensation: value.handicap_compensation,
            total: value.total,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct ScoringView {
    pub node_id: u32,
    pub rule: ScoringRuleView,
    pub dead_stones: Vec<PointView>,
    pub neutral_points: Vec<PointView>,
    /// Row-major, top-left first; None represents a neutral intersection.
    pub ownership: Vec<Option<ColorView>>,
    pub black: ScoringBreakdown,
    pub white: ScoringBreakdown,
    pub score_lead: f64,
    pub result: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "type", content = "data")]
pub enum ScoringError {
    #[error("manual scoring is unavailable during a game, match or coaching session")]
    GameActive,
    #[error("manual scoring has not started")]
    NotStarted,
    #[error("scoring point is outside the board")]
    OutOfBounds,
    #[error("invalid final scoring input")]
    InvalidPosition,
}

pub(crate) enum Request {
    Start(oneshot::Sender<Result<ScoringView, AppError>>),
    Toggle(
        go_core::Point,
        oneshot::Sender<Result<ScoringView, AppError>>,
    ),
    Rule(
        ScoringRuleView,
        oneshot::Sender<Result<ScoringView, AppError>>,
    ),
    Commit(oneshot::Sender<Result<u64, AppError>>),
    Cancel(oneshot::Sender<Result<(), AppError>>),
}

impl AppCore {
    pub async fn start_scoring(&self) -> Result<ScoringView, AppError> {
        let (reply, response) = oneshot::channel();
        self.scoring_request(Request::Start(reply)).await?;
        response.await.map_err(|_| AppError::Closed)?
    }

    pub async fn toggle_scoring_point(
        &self,
        point: go_core::Point,
    ) -> Result<ScoringView, AppError> {
        let (reply, response) = oneshot::channel();
        self.scoring_request(Request::Toggle(point, reply)).await?;
        response.await.map_err(|_| AppError::Closed)?
    }

    pub async fn set_scoring_rule(&self, rule: ScoringRuleView) -> Result<ScoringView, AppError> {
        let (reply, response) = oneshot::channel();
        self.scoring_request(Request::Rule(rule, reply)).await?;
        response.await.map_err(|_| AppError::Closed)?
    }

    pub async fn commit_scoring(&self) -> Result<u64, AppError> {
        let (reply, response) = oneshot::channel();
        self.scoring_request(Request::Commit(reply)).await?;
        response.await.map_err(|_| AppError::Closed)?
    }

    pub async fn cancel_scoring(&self) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.scoring_request(Request::Cancel(reply)).await?;
        response.await.map_err(|_| AppError::Closed)?
    }

    async fn scoring_request(&self, request: Request) -> Result<(), AppError> {
        self.commands
            .send(actor::Command::Scoring(request))
            .await
            .map_err(|_| AppError::Closed)
    }
}
