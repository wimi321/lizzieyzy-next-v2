//! HumanSL work shares the game session and the main engine owner.
use super::Actor;
use crate::{
    AppError, AppEvent, CoachError, CoachMode, CoachSetup, CoachView, GamePhase, coach::*,
    coach_api::Request,
};
use document::{AnalysisKey, GamePosition, NodeId};
use engine_session::{EngineHandle, QueryEvent, QuerySubscription};
use go_core::{Color, Move};
use katago_protocol::AnalysisResponse;
use std::{collections::HashMap, future::pending, time::Duration};
use tokio::{
    sync::{oneshot, watch},
    time::Instant,
};

mod feedback;
mod lifecycle;
mod play;
mod startup;

#[derive(Default)]
pub(super) struct State {
    preparing: bool,
    session: Option<Session>,
    starting: Option<Starting>,
}
impl State {
    pub(super) fn is_starting(&self) -> bool {
        self.preparing || self.starting.is_some()
    }
    pub(super) fn end(&mut self) {
        if let Some(session) = &mut self.session {
            session.active = false;
        }
    }
    pub(super) fn session_active(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.active)
    }
}
struct Starting {
    setup: CoachSetup,
    reply: oneshot::Sender<Result<CoachView, AppError>>,
    status: Option<watch::Receiver<engine_session::EngineStatus>>,
    deadline: Instant,
    phase: StartupPhase,
    error: Option<AppError>,
}
struct Prepared {
    setup: CoachSetup,
    restart: Option<crate::EngineProfile>,
    restore: Option<crate::EngineProfile>,
}
enum StartupPhase {
    Loading {
        restore: Option<crate::EngineProfile>,
    },
    Restoring {
        reason: String,
    },
}
struct Session {
    view: CoachView,
    active: bool,
    ai: Option<AiTurn>,
    review: Option<Query>,
    before: Option<(NodeId, AnalysisResponse)>,
    rating: Option<PendingRating>,
    ratings: Vec<crate::CoachMoveRated>,
    hints: HashMap<AnalysisKey, u8>,
    low_winrates: u32,
}
struct Query {
    engine: EngineHandle,
    subscription: QuerySubscription,
    node: NodeId,
    key: AnalysisKey,
    deadline: Instant,
    latest: Option<AnalysisResponse>,
}
struct AiTurn {
    position: GamePosition,
    node: NodeId,
    play: crate::HumanSlPlay,
    query: Option<Query>,
    stage: Stage,
    started: Instant,
    deadline: Instant,
    policy: Option<AnalysisResponse>,
    pool: Vec<PolicyCandidate>,
    round: usize,
    last_visits: u64,
    query_started: Instant,
    pass_checked: bool,
    selected: Option<Move>,
    ready_at: Option<Instant>,
}
#[derive(Clone, Copy)]
enum Stage {
    Policy,
    Search,
    Pass,
}
struct PendingRating {
    before: NodeId,
    position: GamePosition,
    mv: Move,
    node: NodeId,
    move_number: u32,
    deadline: Instant,
}
pub(super) enum Event {
    Started(Result<(), AppError>),
    Ai(Box<Option<QueryEvent>>),
    Review(Box<Option<QueryEvent>>),
    Tick,
}
async fn receive_query(query: &mut Option<Query>) -> Option<QueryEvent> {
    match query {
        Some(query) => query.subscription.recv().await,
        None => pending().await,
    }
}
pub(super) async fn receive(state: &mut State) -> Event {
    if let Some(starting) = &mut state.starting {
        return Event::Started(startup::receive(starting).await);
    }
    let Some(session) = &mut state.session else {
        return pending().await;
    };
    let deadline = session
        .review
        .as_ref()
        .map(|q| q.deadline)
        .into_iter()
        .chain(session.rating.as_ref().map(|r| r.deadline))
        .chain(
            session
                .ai
                .as_ref()
                .and_then(|ai| ai.query.as_ref().map(|q| q.deadline)),
        )
        .chain(session.ai.as_ref().and_then(|ai| {
            (session.rating.is_none() && !session.view.awaiting_decision)
                .then_some(ai.ready_at)
                .flatten()
        }))
        .min();
    tokio::select! {
        event = async { match &mut session.ai {Some(ai)=>receive_query(&mut ai.query).await, None=>pending().await} } => Event::Ai(Box::new(event)),
        event = receive_query(&mut session.review) => Event::Review(Box::new(event)),
        () = async {match deadline {Some(at)=>tokio::time::sleep_until(at).await,None=>pending().await}} => Event::Tick,
    }
}
impl Actor {
    pub(super) fn coach_view(&self) -> Option<CoachView> {
        self.coach.session.as_ref().map(|s| {
            let mut view = s.view.clone();
            view.active = s.active;
            view
        })
    }
    pub(super) fn publish_coach(&self) {
        if let Some(view) = self.coach_view() {
            self.output.send(AppEvent::CoachUpdated(view));
        }
    }
    pub(super) fn coach_hides_analysis(&self) -> bool {
        self.coach.preparing
            || self.coach.starting.is_some()
            || self
                .coach
                .session
                .as_ref()
                .is_some_and(|s| s.active && s.view.setup.mode != CoachMode::LiveAnalysis)
    }
    pub(super) fn coach_thinking(&self) -> bool {
        self.coach.session.as_ref().is_some_and(|s| s.ai.is_some())
    }
    fn coach_session(&self) -> Result<&Session, CoachError> {
        self.coach
            .session
            .as_ref()
            .filter(|s| s.active)
            .ok_or(CoachError::NotActive)
    }
    pub(super) async fn coach_command(&mut self, request: Request) {
        if self.coach.is_starting() {
            match request {
                Request::View(reply) => {
                    let _ = reply.send(self.coach_view());
                }
                Request::Start(_, reply) => {
                    let _ = reply.send(Err(crate::GameError::AlreadyActive.into()));
                }
                Request::Hint(reply) | Request::Decision(_, reply) => {
                    let _ = reply.send(Err(AppError::GameRunning));
                }
                Request::Review(reply) => {
                    let _ = reply.send(Err(AppError::GameRunning));
                }
            }
            return;
        }
        match request {
            Request::Start(setup, reply) => {
                self.coach.preparing = true;
                let prepared = self.prepare_coach(setup).await;
                self.coach.preparing = false;
                match prepared {
                    Ok(prepared) => self.begin_coach_start(prepared, reply).await,
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
            }
            Request::Hint(reply) => {
                let result = self.coach_hint();
                let _ = reply.send(result);
            }
            Request::Decision(decision, reply) => {
                let result = self.coach_decision(decision).await;
                let _ = reply.send(result);
            }
            Request::Review(reply) => {
                let result = self.coach_review().await;
                let _ = reply.send(result);
            }
            Request::View(reply) => {
                let _ = reply.send(self.coach_view());
            }
        }
    }
    pub(super) async fn coach_signal(&mut self, event: Event) {
        if let Event::Started(result) = event {
            self.finish_coach_start(result).await;
            return;
        }
        let result = match event {
            Event::Ai(event) => self.coach_ai_event(*event).await,
            Event::Review(event) => self.coach_review_event(*event).await,
            Event::Tick => self.coach_tick().await,
            Event::Started(_) => Ok(()),
        };
        if let Err(error) = result {
            let _ = self.pause_game().await;
            if let Some(game) = &mut self.game {
                game.view.error = Some(crate::GameError::Engine(error.to_string()));
            }
            self.publish_game();
        }
    }
    async fn cancel_coach_query(query: Option<Query>) {
        if let Some(query) = query {
            let _ = query
                .engine
                .terminate(query.subscription.id().to_owned())
                .await;
        }
    }
    pub(super) async fn cancel_coach_queries(&mut self) {
        if let Some(session) = &mut self.coach.session {
            let ai = session.ai.take().and_then(|ai| ai.query);
            let review = session.review.take();
            session.rating = None;
            Self::cancel_coach_query(ai).await;
            Self::cancel_coach_query(review).await;
        }
    }
    async fn coach_tick(&mut self) -> Result<(), AppError> {
        let now = Instant::now();
        if let Some(session) = &mut self.coach.session {
            if session.rating.as_ref().is_some_and(|r| now >= r.deadline) {
                session.rating = None;
                Self::cancel_coach_query(session.review.take()).await;
            }
            if session.review.as_ref().is_some_and(|q| now >= q.deadline) {
                let query = session.review.take();
                let latest = query
                    .as_ref()
                    .and_then(|q| q.latest.clone().map(|r| (q.node, r)));
                Self::cancel_coach_query(query).await;
                if let Some((node, response)) = latest {
                    self.finish_coach_review(node, response).await?;
                }
            }
        }
        let timed_out = self
            .coach
            .session
            .as_ref()
            .and_then(|s| s.ai.as_ref())
            .and_then(|ai| ai.query.as_ref())
            .is_some_and(|q| now >= q.deadline);
        if timed_out {
            self.coach_ai_timeout().await?;
        }
        self.maybe_coach_move().await
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod api_tests;
