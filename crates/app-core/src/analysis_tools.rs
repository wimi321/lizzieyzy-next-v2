//! Research controls are serialized by the document actor.
use crate::{AppCore, AppError, PointView, actor};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum RestrictionKind {
    Allow,
    Avoid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum RestrictionDepth {
    FirstMove,
    Custom(u16),
    WholeSearch,
}

impl Default for RestrictionDepth {
    fn default() -> Self {
        Self::Custom(5)
    }
}
impl RestrictionDepth {
    pub fn validate(self) -> Result<(), AnalysisToolsError> {
        if matches!(self, Self::Custom(n) if !(1..=200).contains(&n)) {
            return Err(AnalysisToolsError::InvalidDepth);
        }
        Ok(())
    }
    pub(crate) fn value(self) -> u32 {
        match self {
            Self::FirstMove => 1,
            Self::Custom(n) => u32::from(n),
            Self::WholeSearch => 1_000_000,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct RestrictionDepthSettings {
    #[ts(as = "Option<RestrictionDepth>", optional)]
    pub allow: RestrictionDepth,
    #[ts(as = "Option<RestrictionDepth>", optional)]
    pub avoid: RestrictionDepth,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AnalysisRestriction {
    pub kind: RestrictionKind,
    pub points: Vec<PointView>,
    pub depth: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct SearchParams {
    #[ts(optional = nullable)]
    pub playout_doubling_advantage: Option<f64>,
    #[ts(optional = nullable)]
    pub wide_root_noise: Option<f64>,
    #[ts(optional = nullable)]
    pub root_policy_temperature: Option<f64>,
}

impl SearchParams {
    pub fn validate(&self) -> Result<(), AnalysisToolsError> {
        for (name, value, min, max) in [
            (
                "playoutDoublingAdvantage",
                self.playout_doubling_advantage,
                -3.0,
                3.0,
            ),
            ("wideRootNoise", self.wide_root_noise, 0.0, 5.0),
            (
                "rootPolicyTemperature",
                self.root_policy_temperature,
                0.01,
                100.0,
            ),
        ] {
            if value.is_some_and(|v| !v.is_finite() || !(min..=max).contains(&v)) {
                return Err(AnalysisToolsError::InvalidSearchParam(name.into()));
            }
        }
        Ok(())
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.playout_doubling_advantage.is_none()
            && self.wide_root_noise.is_none()
            && self.root_policy_temperature.is_none()
    }
}
impl From<&SearchParams> for analysis::SearchParams {
    fn from(p: &SearchParams) -> Self {
        Self {
            playout_doubling_advantage: p.playout_doubling_advantage,
            wide_root_noise: p.wide_root_noise,
            root_policy_temperature: p.root_policy_temperature,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum AnalysisToolsError {
    #[error("restriction depth must be within 1..=200")]
    InvalidDepth,
    #[error("restriction point is outside the board")]
    InvalidPoint,
    #[error("invalid search parameter: {0}")]
    InvalidSearchParam(String),
    #[error("research tools are unavailable during a game, match or coaching session")]
    GameActive,
    #[error("the current position is not a proven ladder")]
    NotLadder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum LadderOutcome {
    Captured,
    Escaped,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LadderContinuation {
    pub outcome: LadderOutcome,
    pub moves: usize,
}

pub(crate) enum Request {
    Set(
        RestrictionKind,
        Vec<PointView>,
        oneshot::Sender<Result<Option<AnalysisRestriction>, AppError>>,
    ),
    Add(
        RestrictionKind,
        Vec<PointView>,
        oneshot::Sender<Result<Option<AnalysisRestriction>, AppError>>,
    ),
    Remove(
        Vec<PointView>,
        oneshot::Sender<Result<Option<AnalysisRestriction>, AppError>>,
    ),
    Clear(oneshot::Sender<Result<(), AppError>>),
    Depth(
        RestrictionKind,
        RestrictionDepth,
        oneshot::Sender<Result<(), AppError>>,
    ),
    Search(
        Option<SearchParams>,
        oneshot::Sender<Result<Option<SearchParams>, AppError>>,
    ),
    Ladder(oneshot::Sender<Result<LadderContinuation, AppError>>),
}

impl AppCore {
    async fn analysis_tool<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<Result<T, AppError>>) -> Request,
    ) -> Result<T, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::AnalysisTools(make(reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
    pub async fn set_analysis_restriction(
        &self,
        kind: RestrictionKind,
        points: Vec<PointView>,
    ) -> Result<Option<AnalysisRestriction>, AppError> {
        self.analysis_tool(|r| Request::Set(kind, points, r)).await
    }
    pub async fn add_restriction_points(
        &self,
        kind: RestrictionKind,
        points: Vec<PointView>,
    ) -> Result<Option<AnalysisRestriction>, AppError> {
        self.analysis_tool(|r| Request::Add(kind, points, r)).await
    }
    pub async fn remove_restriction_points(
        &self,
        points: Vec<PointView>,
    ) -> Result<Option<AnalysisRestriction>, AppError> {
        self.analysis_tool(|r| Request::Remove(points, r)).await
    }
    pub async fn clear_analysis_restriction(&self) -> Result<(), AppError> {
        self.analysis_tool(Request::Clear).await
    }
    pub async fn set_restriction_depth(
        &self,
        kind: RestrictionKind,
        depth: RestrictionDepth,
    ) -> Result<(), AppError> {
        self.analysis_tool(|r| Request::Depth(kind, depth, r)).await
    }
    pub async fn search_params(&self) -> Result<Option<SearchParams>, AppError> {
        Ok(self.settings().await?.search_params)
    }
    pub async fn set_search_params(
        &self,
        params: Option<SearchParams>,
    ) -> Result<Option<SearchParams>, AppError> {
        self.analysis_tool(|r| Request::Search(params, r)).await
    }
    pub async fn continue_ladder(&self) -> Result<LadderContinuation, AppError> {
        self.analysis_tool(Request::Ladder).await
    }
}
