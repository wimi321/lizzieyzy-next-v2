use crate::{
    AnalysisConfig, AnalysisError, AnalysisLimits, AnalysisSnapshot, AnalysisStatus, Focus,
    LimitReason, LiveSettings, SnapshotSource, cache::Cache, delivery::Delivery, request,
    speed::SearchSpeed, throttle::Throttle,
};
use document::{AnalysisKey, NodeId, Revision};
use engine_session::{EngineHandle, QueryEvent, QuerySubscription};
use std::{future::pending, num::NonZeroUsize};
use tokio::{
    sync::{mpsc, oneshot, watch},
    time::{Instant, sleep_until},
};

pub(crate) enum Input {
    Focus(
        Box<Focus>,
        Option<Box<katago_protocol::AnalysisResponse>>,
        bool,
    ),
    SetPondering(bool),
    SetCacheEnabled(bool),
    DiscardCachedAnalysis,
    Resume,
    SetLimits(AnalysisLimits),
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
    started: Option<Instant>,
    visits: Option<u64>,
}

struct Actor {
    engine: EngineHandle,
    config: AnalysisConfig,
    focus: Option<Focus>,
    active: Option<Active>,
    limited: Option<LimitReason>,
    limited_on_current: bool,
    playout_exempt: bool,
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
        limited: None,
        limited_on_current: false,
        playout_exempt: false,
        cache: Cache::new(capacity),
        throttle: Throttle::new(),
        last_published: None,
        display_floor: None,
        status,
        output,
    };
    loop {
        let deadline = actor.throttle.deadline();
        let limit_deadline = actor
            .active
            .as_ref()
            .and_then(|active| actor.config.limits.deadline(active.started));
        tokio::select! {
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
            () = tick(limit_deadline) => actor.stop_by_limit(LimitReason::Time).await,
            event = receive(&mut actor.active) => {
                if let Some(reason) = actor.event(event) {
                    actor.stop_by_limit(reason).await;
                }
            }
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
            Input::Focus(focus, cached, resume_limited) => {
                // Validate before invalidating the currently usable focus.
                request::build(&focus.position, &self.config.live)?;
                let same_position = self.focus.as_ref().is_some_and(|current| {
                    current.node_id == focus.node_id && current.key == focus.key
                });
                let resumed = resume_limited && self.limited.is_some();
                if resumed {
                    self.playout_exempt |= same_position
                        && self.limited_on_current
                        && self.limited == Some(LimitReason::Playouts);
                    self.limited = None;
                    self.limited_on_current = false;
                    self.status.send_replace(AnalysisStatus::Idle);
                }
                if self.config.cache_enabled
                    && (self.last_published.is_none()
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
                        && (resumed || matches!(*self.status.borrow(), AnalysisStatus::Failed(_)))
                    {
                        return self.start().await;
                    }
                    return Ok(());
                }
                if same_position {
                    self.rebase_focus(*focus);
                    if self.last_published.is_none() {
                        self.offer_cached();
                    }
                    return if resumed { self.start().await } else { Ok(()) };
                }
                self.output.clear();
                self.throttle.reset();
                self.last_published = None;
                self.display_floor = None;
                self.playout_exempt = false;
                self.limited_on_current = false;
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
                self.playout_exempt |=
                    self.limited_on_current && self.limited == Some(LimitReason::Playouts);
                self.limited = None;
                self.limited_on_current = false;
                if !enabled {
                    self.output.clear();
                }
                self.cancel().await?;
                self.start().await
            }
            Input::SetCacheEnabled(enabled) => {
                self.config.cache_enabled = enabled;
                Ok(())
            }
            Input::DiscardCachedAnalysis => {
                self.cache.clear();
                self.output.clear();
                self.throttle.reset();
                self.last_published = None;
                self.display_floor = None;
                self.cancel().await?;
                self.start().await
            }
            Input::Resume => {
                self.playout_exempt |=
                    self.limited_on_current && self.limited == Some(LimitReason::Playouts);
                self.limited_on_current = false;
                if self.limited.take().is_some() {
                    self.status.send_replace(AnalysisStatus::Idle);
                    self.start().await?;
                }
                Ok(())
            }
            Input::SetLimits(limits) => {
                self.config.limits = limits;
                if let Some(reason) = self.limit_reached() {
                    self.stop_by_limit(reason).await;
                }
                Ok(())
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
        if self.config.cache_enabled
            && let Some(focus) = &self.focus
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
        self.status.send_replace(
            self.limited
                .map_or(AnalysisStatus::Idle, AnalysisStatus::Limited),
        );
        Ok(())
    }

    async fn start(&mut self) -> Result<(), AnalysisError> {
        if self.config.pondering
            && self.limited.is_none()
            && let Some(focus) = &self.focus
        {
            let request = request::build(&focus.position, &self.config.live)?;
            let subscription = self.engine.submit(request).await?;
            self.active = Some(Active {
                identity: Identity::from(focus),
                subscription,
                speed: SearchSpeed::default(),
                started: None,
                visits: None,
            });
            self.status.send_replace(AnalysisStatus::Analyzing);
        }
        Ok(())
    }

    fn event(&mut self, event: Option<QueryEvent>) -> Option<LimitReason> {
        let Some(active) = &mut self.active else {
            return None;
        };
        let identity = active.identity;
        if self.focus.as_ref().map(Identity::from) != Some(identity) {
            return None;
        }
        match event {
            Some(QueryEvent::Partial(response) | QueryEvent::Final(response)) => {
                let final_result = !response.is_during_search;
                // Queueing and engine startup are not live search time. The
                // first result is the protocol's first proof of actual work.
                active.started.get_or_insert_with(Instant::now);
                active.visits = Some(response.root_info.visits);
                let reason = self.config.limits.reached(
                    active.started,
                    (!self.playout_exempt).then_some(active.visits).flatten(),
                );
                // Measure every received Partial, before display-depth filtering
                // or throttling. A Final retains the last measured speed.
                if !final_result {
                    active.speed.record(response.root_info.visits);
                }
                let visits_per_second = active.speed.visits_per_second();
                if self.config.cache_enabled {
                    self.cache.insert(
                        identity.key,
                        &self.config.engine_fingerprint,
                        response.clone(),
                    );
                }
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
                return reason;
            }
            Some(QueryEvent::Restarted) => {
                active.speed = SearchSpeed::default();
                active.started = None;
                active.visits = None;
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
        None
    }

    async fn stop_by_limit(&mut self, reason: LimitReason) {
        self.limited = Some(reason);
        self.limited_on_current = true;
        // Do not use cancel(): it clears pending results. The latest accepted
        // snapshot must still be delivered through the usual trailing throttle.
        if let Some(active) = self.active.take()
            && let Err(error) = self
                .engine
                .terminate(active.subscription.id().to_owned())
                .await
        {
            self.status
                .send_replace(AnalysisStatus::Failed(error.into()));
            return;
        }
        self.status.send_replace(AnalysisStatus::Limited(reason));
    }

    fn limit_reached(&self) -> Option<LimitReason> {
        self.active.as_ref().and_then(|active| {
            self.config.limits.reached(
                active.started,
                (!self.playout_exempt).then_some(active.visits).flatten(),
            )
        })
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
