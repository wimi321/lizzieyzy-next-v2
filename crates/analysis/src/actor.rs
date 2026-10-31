use crate::{
    AnalysisConfig, AnalysisError, AnalysisSnapshot, AnalysisStatus, Focus, LiveSettings,
    SnapshotSource, cache::Cache, delivery::Delivery, request, speed::SearchSpeed,
    throttle::Throttle,
};
use document::{AnalysisKey, NodeId, Revision};
use engine_session::{EngineHandle, QueryEvent, QuerySubscription};
use std::{future::pending, num::NonZeroUsize};
use tokio::{
    sync::{mpsc, oneshot, watch},
    time::{Instant, sleep_until},
};

pub(crate) enum Input {
    Focus(Box<Focus>, Option<Box<katago_protocol::AnalysisResponse>>),
    SetPondering(bool),
    SetLiveSettings(LiveSettings),
    Shutdown,
}

pub(crate) struct Command {
    pub input: Input,
    pub reply: oneshot::Sender<Result<(), AnalysisError>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    revision: Revision,
    node_id: NodeId,
    key: AnalysisKey,
}

impl From<&Focus> for Identity {
    fn from(focus: &Focus) -> Self {
        Self {
            revision: focus.revision,
            node_id: focus.node_id,
            key: focus.key,
        }
    }
}

struct Active {
    identity: Identity,
    subscription: QuerySubscription,
    speed: SearchSpeed,
}

struct Actor {
    engine: EngineHandle,
    config: AnalysisConfig,
    focus: Option<Focus>,
    active: Option<Active>,
    cache: Cache,
    throttle: Throttle,
    last_published: Option<AnalysisSnapshot>,
    display_floor: Option<u64>,
    status: watch::Sender<AnalysisStatus>,
    output: Delivery,
}

pub(crate) async fn run(
    engine: EngineHandle,
    config: AnalysisConfig,
    capacity: NonZeroUsize,
    mut commands: mpsc::Receiver<Command>,
    status: watch::Sender<AnalysisStatus>,
    output: Delivery,
) {
    let mut actor = Actor {
        engine,
        config,
        focus: None,
        active: None,
        cache: Cache::new(capacity),
        throttle: Throttle::new(),
        last_published: None,
        display_floor: None,
        status,
        output,
    };
    loop {
        let deadline = actor.throttle.deadline();
        tokio::select! {
            biased;
            command = commands.recv() => {
                let Some(command) = command else { break };
                let stop = matches!(command.input, Input::Shutdown);
                let result = actor.command(command.input).await;
                if !stop && let Err(error @ AnalysisError::Session(_)) = &result {
                    actor.status.send_replace(AnalysisStatus::Failed(error.clone()));
                }
                let _ = command.reply.send(result);
                if stop { return; }
            }
            event = receive(&mut actor.active) => actor.event(event),
            () = tick(deadline) => {
                if let Some(snapshot) = actor.throttle.flush(Instant::now()) {
                    actor.publish(snapshot);
                }
            }
        }
    }
    let _ = actor.cancel().await;
    actor.output.clear();
    actor.status.send_replace(AnalysisStatus::Stopped);
}

async fn receive(active: &mut Option<Active>) -> Option<QueryEvent> {
    match active {
        Some(active) => active.subscription.recv().await,
        None => pending().await,
    }
}

async fn tick(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => pending().await,
    }
}

impl Actor {
    async fn command(&mut self, input: Input) -> Result<(), AnalysisError> {
        match input {
            Input::Focus(focus, cached) => {
                // Validate before invalidating the currently usable focus.
                request::build(&focus.position, &self.config.live)?;
                if (self.last_published.is_none()
                    || self.focus.as_ref().is_none_or(|current| {
                        current.node_id != focus.node_id || current.key != focus.key
                    }))
                    && let Some(response) = cached
                {
                    self.cache
                        .insert(focus.key, &self.config.engine_fingerprint, *response);
                }
                if self.focus.as_ref().map(Identity::from) == Some(Identity::from(focus.as_ref())) {
                    if self.last_published.is_none() {
                        self.offer_cached();
                    }
                    if self.active.is_none()
                        && matches!(*self.status.borrow(), AnalysisStatus::Failed(_))
                    {
                        return self.start().await;
                    }
                    return Ok(());
                }
                if self.focus.as_ref().is_some_and(|current| {
                    current.node_id == focus.node_id && current.key == focus.key
                }) {
                    self.rebase_focus(*focus);
                    if self.last_published.is_none() {
                        self.offer_cached();
                    }
                    return Ok(());
                }
                self.output.clear();
                self.throttle.reset();
                self.last_published = None;
                self.display_floor = None;
                self.focus = Some(*focus);
                // Cancel by stable subscription id, including across restarts.
                self.cancel().await?;
                self.offer_cached();
                self.start().await
            }
            Input::SetPondering(enabled) => {
                if self.config.pondering == enabled {
                    return Ok(());
                }
                self.config.pondering = enabled;
                if !enabled {
                    self.output.clear();
                }
                self.cancel().await?;
                self.start().await
            }
            Input::SetLiveSettings(settings) => {
                if self.config.live == settings {
                    return Ok(());
                }
                self.config.live = settings;
                self.output.clear();
                self.last_published = None;
                self.display_floor = None;
                self.cancel().await?;
                self.start().await
            }
            Input::Shutdown => {
                self.output.clear();
                let result = self.cancel().await;
                self.status.send_replace(AnalysisStatus::Stopped);
                result
            }
        }
    }

    fn offer_cached(&mut self) {
        if let Some(focus) = &self.focus
            && let Some(response) = self.cache.get(focus.key, &self.config.engine_fingerprint)
        {
            self.offer(AnalysisSnapshot {
                revision: focus.revision,
                node_id: focus.node_id,
                key: focus.key,
                response,
                source: SnapshotSource::Cache,
                visits_per_second: None,
            });
        }
    }

    fn rebase_focus(&mut self, focus: Focus) {
        let identity = Identity::from(&focus);
        self.focus = Some(focus);
        if let Some(active) = &mut self.active {
            active.identity = identity;
        }
        // Replace unread old-revision data as well as future query identities.
        // Keep pending results: a throttled Final may have no active query left.
        self.output.clear();
        let latest = self.last_published.clone().map(|mut snapshot| {
            snapshot.revision = identity.revision;
            snapshot
        });
        self.throttle
            .rebase_revision(identity.revision, Instant::now(), latest.is_some());
        if let Some(snapshot) = latest {
            // An identity refresh is immediate, even inside the old revision's
            // throttle interval. The next live update still waits a full 50ms.
            // Preserve the floor: recovery may still be displaying an old
            // process's snapshot while waiting for its first new result.
            self.last_published = Some(snapshot.clone());
            self.output.send(snapshot);
        }
    }

    async fn cancel(&mut self) -> Result<(), AnalysisError> {
        self.throttle.clear_pending();
        if let Some(active) = self.active.take() {
            self.engine
                .terminate(active.subscription.id().to_owned())
                .await?;
        }
        self.status.send_replace(AnalysisStatus::Idle);
        Ok(())
    }

    async fn start(&mut self) -> Result<(), AnalysisError> {
        if self.config.pondering
            && let Some(focus) = &self.focus
        {
            let request = request::build(&focus.position, &self.config.live)?;
            let subscription = self.engine.submit(request).await?;
            self.active = Some(Active {
                identity: Identity::from(focus),
                subscription,
                speed: SearchSpeed::default(),
            });
            self.status.send_replace(AnalysisStatus::Analyzing);
        }
        Ok(())
    }

    fn event(&mut self, event: Option<QueryEvent>) {
        let Some(active) = &mut self.active else {
            return;
        };
        let identity = active.identity;
        if self.focus.as_ref().map(Identity::from) != Some(identity) {
            return;
        }
        match event {
            Some(QueryEvent::Partial(response) | QueryEvent::Final(response)) => {
                let final_result = !response.is_during_search;
                // Measure every received Partial, before display-depth filtering
                // or throttling. A Final retains the last measured speed.
                if !final_result {
                    active.speed.record(response.root_info.visits);
                }
                let visits_per_second = active.speed.visits_per_second();
                self.cache.insert(
                    identity.key,
                    &self.config.engine_fingerprint,
                    response.clone(),
                );
                self.offer(AnalysisSnapshot {
                    revision: identity.revision,
                    node_id: identity.node_id,
                    key: identity.key,
                    response,
                    source: SnapshotSource::Live,
                    visits_per_second,
                });
                if final_result {
                    self.active = None;
                    self.status.send_replace(AnalysisStatus::Idle);
                } else {
                    self.status.send_replace(AnalysisStatus::Analyzing);
                }
            }
            Some(QueryEvent::Restarted) => {
                active.speed = SearchSpeed::default();
                self.throttle.clear_pending();
                // Keep the old display until the new process reports progress.
                self.display_floor = None;
                // engine-session replays this very subscription; never submit twice.
                self.status.send_replace(AnalysisStatus::Restarting);
            }
            Some(QueryEvent::Failed(error)) => {
                self.active = None;
                self.throttle.clear_pending();
                self.status
                    .send_replace(AnalysisStatus::Failed(error.into()));
            }
            Some(QueryEvent::Terminated) | None => {
                self.active = None;
                self.throttle.clear_pending();
                self.status.send_replace(AnalysisStatus::Idle);
            }
        }
    }

    fn offer(&mut self, snapshot: AnalysisSnapshot) {
        if self.is_shallower(&snapshot) {
            return;
        }
        if let Some(snapshot) = self.throttle.push(snapshot, Instant::now()) {
            self.publish(snapshot);
        }
    }

    fn is_shallower(&self, snapshot: &AnalysisSnapshot) -> bool {
        self.display_floor
            .is_some_and(|floor| snapshot.response.root_info.visits < floor)
    }

    fn publish(&mut self, snapshot: AnalysisSnapshot) {
        if self.focus.as_ref().is_some_and(|focus| {
            focus.revision == snapshot.revision
                && focus.node_id == snapshot.node_id
                && focus.key == snapshot.key
        }) && !self.is_shallower(&snapshot)
        {
            self.display_floor = Some(snapshot.response.root_info.visits);
            self.last_published = Some(snapshot.clone());
            self.output.send(snapshot);
        }
    }
}

#[cfg(test)]
mod tests;
