use crate::{
    AnalysisRequest, EngineStatus, QueryError, QueryEvent, QueryId, QuerySubscription,
    SessionConfig, SessionError, TransportSpec, delivery, transport,
};
use katago_protocol::{Action, ActionResponse, EngineLine, Request, parse_line};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, sleep_until};

const MAX_PENDING: usize = 128;
const MAX_OUTGOING: usize = 256;

pub(crate) enum Command {
    Submit(
        Box<AnalysisRequest>,
        oneshot::Sender<Result<QuerySubscription, SessionError>>,
    ),
    Terminate(QueryId, oneshot::Sender<Result<(), SessionError>>),
    Shutdown(oneshot::Sender<Result<(), SessionError>>),
}

struct Pending {
    ordinal: u64,
    request: AnalysisRequest,
    remaining: BTreeSet<u32>,
    wire_id: QueryId,
    delivery: delivery::Delivery,
    started: Instant,
    submitted: bool,
}

struct Actor {
    cfg: SessionConfig,
    commands: mpsc::Receiver<Command>,
    status: watch::Sender<EngineStatus>,
    pending: BTreeMap<QueryId, Pending>,
    outgoing: VecDeque<(Option<QueryId>, String)>,
    instance: uuid::Uuid,
    sequence: u64,
    attempt: u32,
    ready: bool,
    version_id: QueryId,
    last_output: Instant,
    bad_lines: u32,
    total_bad_lines: u64,
    shutdown: Option<oneshot::Sender<Result<(), SessionError>>>,
    stopping: bool,
    cleanup_error: Option<String>,
}

enum Outcome {
    Stop,
    Fault { reason: String, fatal: bool },
    Exited(transport::Exit),
}

pub(crate) async fn run(
    spec: TransportSpec,
    cfg: SessionConfig,
    commands: mpsc::Receiver<Command>,
    status: watch::Sender<EngineStatus>,
) {
    let mut actor = Actor {
        cfg,
        commands,
        status,
        pending: BTreeMap::new(),
        outgoing: VecDeque::new(),
        instance: uuid::Uuid::new_v4(),
        sequence: 0,
        attempt: 0,
        ready: false,
        version_id: String::new(),
        last_output: Instant::now(),
        bad_lines: 0,
        total_bad_lines: 0,
        shutdown: None,
        stopping: false,
        cleanup_error: None,
    };
    if let Err(error) = actor.cfg.validate() {
        actor.fail(error.to_string(), Vec::new());
        actor.wait_failed().await;
    } else {
        actor.run_processes(spec).await;
    }
    actor.fail_queries(QueryError::Shutdown);
    if actor.cleanup_error.is_none() {
        actor.status.send_replace(EngineStatus::Stopped);
    }
    if let Some(reply) = actor.shutdown.take() {
        let _ = reply.send(
            actor
                .cleanup_error
                .map_or(Ok(()), |error| Err(SessionError::Cleanup(error))),
        );
    }
}

impl Actor {
    fn next_id(&mut self) -> QueryId {
        // Every process generation shares this counter; neither actions nor
        // restarted queries can reuse a previous wire identity.
        self.sequence += 1;
        format!("{}-{}", self.instance, self.sequence)
    }

    async fn run_processes(&mut self, spec: TransportSpec) {
        loop {
            let mut process = match transport::start(&spec, &self.cfg) {
                Ok(process) => process,
                Err(error) => {
                    self.fail(format!("could not start engine: {error}"), Vec::new());
                    self.wait_failed().await;
                    return;
                }
            };
            self.ready = false;
            self.bad_lines = 0;
            self.outgoing.clear();
            self.version_id = self.next_id();
            self.queue_action(Action::QueryVersion {
                id: self.version_id.clone(),
            });
            let outcome = self.drive(&mut process).await;
            let (reason, fatal, exit) = match outcome {
                Outcome::Exited(exit) => {
                    let reason = match &exit.status {
                        Ok(status) => format!("engine exited: {status}"),
                        Err(error) => error.clone(),
                    };
                    (reason, !self.ready, exit)
                }
                Outcome::Fault { reason, fatal } => {
                    let exit = self.finish_transport(&mut process).await;
                    (reason, fatal, exit)
                }
                Outcome::Stop => {
                    self.fail_queries(QueryError::Shutdown);
                    let exit = self.finish_transport(&mut process).await;
                    if let Err(error) = exit.status {
                        self.fail(format!("process cleanup failed: {error}"), exit.stderr_tail);
                        self.cleanup_error = Some(error);
                    }
                    return;
                }
            };
            self.outgoing.clear();
            if let Err(error) = exit.status {
                self.fail(format!("{reason}; {error}"), exit.stderr_tail);
                self.cleanup_error = Some(error);
                self.wait_failed().await;
                return;
            }
            if fatal || self.attempt >= self.cfg.max_restarts {
                self.fail(reason, exit.stderr_tail);
                self.wait_failed().await;
                return;
            }
            self.attempt += 1;
            self.ready = false;
            self.status.send_replace(EngineStatus::Restarting {
                attempt: self.attempt,
                reason: reason.clone(),
            });
            tracing::warn!(attempt = self.attempt, %reason, "restarting engine");
            let delay = self
                .cfg
                .restart_backoff
                .saturating_mul(1u32 << (self.attempt - 1))
                .min(self.cfg.max_restart_backoff);
            let deadline = Instant::now() + delay;
            loop {
                tokio::select! {
                    _ = sleep_until(deadline) => break,
                    command = self.commands.recv() => {
                        self.command(command);
                        if self.stopping { return; }
                    }
                }
            }
        }
    }

    async fn drive(&mut self, process: &mut transport::Transport) -> Outcome {
        let startup_deadline = Instant::now() + self.cfg.startup_timeout;
        let mut driver_done = false;
        let mut input_open = true;
        loop {
            let deadline = if self.ready {
                self.last_output + self.cfg.response_timeout
            } else {
                startup_deadline
            };
            tokio::select! {
                _ = &mut process.driver, if !driver_done => { driver_done = true; }
                output = process.incoming.recv(), if input_open => {
                    match output {
                        Some(output) => if let Some(fault) = self.output(output, true) { return fault; },
                        None => input_open = false,
                    }
                }
                exit = &mut process.exited => {
                    // The exit channel and line channel have independent receive
                    // order. All already-enqueued final lines must be applied.
                    while let Ok(output) = process.incoming.try_recv() { self.output(output, true); }
                    return Outcome::Exited(exit.unwrap_or_else(|_| transport::Exit {
                        status: Err("transport exited without an exit report".into()), stderr_tail: Vec::new(),
                    }));
                }
                command = self.commands.recv() => {
                    self.command(command);
                    if self.stopping { return Outcome::Stop; }
                }
                permit = process.outgoing.reserve(), if !self.outgoing.is_empty() => {
                    match permit {
                        Ok(permit) => if let Some((_, line)) = self.outgoing.pop_front() { permit.send(line); },
                        Err(_) => {
                            // A closed stdin can race the final output/exit.
                            // Let the transport finish draining instead of retrying early.
                            self.outgoing.clear();
                        }
                    }
                }
                _ = sleep_until(deadline), if !self.ready || !self.pending.is_empty() => {
                    return Outcome::Fault {
                        reason: if self.ready { "response watchdog timed out" } else { "startup query_version timed out" }.into(),
                        fatal: false,
                    };
                }
            }
        }
    }

    fn command(&mut self, command: Option<Command>) {
        match command {
            Some(Command::Submit(request, reply)) => {
                if reply.is_closed() {
                    return;
                }
                if let EngineStatus::Failed { reason, .. } = &*self.status.borrow() {
                    let _ = reply.send(Err(SessionError::Unavailable(reason.clone())));
                    return;
                }
                if self.pending.len() >= MAX_PENDING || self.outgoing.len() >= MAX_OUTGOING {
                    let _ = reply.send(Err(SessionError::Capacity));
                    return;
                }
                let id = self.next_id();
                let (delivery, subscription) = delivery::channel(id.clone());
                let was_idle = self.pending.is_empty();
                self.pending.insert(
                    id.clone(),
                    Pending {
                        ordinal: self.sequence,
                        remaining: request.turns(),
                        request: *request,
                        wire_id: id.clone(),
                        delivery,
                        started: Instant::now(),
                        submitted: false,
                    },
                );
                if was_idle {
                    self.last_output = Instant::now();
                }
                if self.ready {
                    self.submit_pending(&id);
                }
                tracing::debug!(query_id = %id, "query submitted");
                if reply.send(Ok(subscription)).is_err() {
                    self.cancel(&id);
                }
            }
            Some(Command::Terminate(id, reply)) => {
                // Reserve before mutating so cancellation is never silently lost
                // when a blocked engine fills the bounded command queue.
                if self.pending.contains_key(&id) && self.outgoing.len() >= MAX_OUTGOING {
                    let _ = reply.send(Err(SessionError::Capacity));
                } else {
                    self.cancel(&id);
                    let _ = reply.send(Ok(()));
                }
            }
            Some(Command::Shutdown(reply)) => {
                self.shutdown = Some(reply);
                self.stopping = true;
            }
            None => self.stopping = true,
        }
    }

    fn cancel(&mut self, id: &str) {
        if let Some(pending) = self.pending.remove(id) {
            self.outgoing
                .retain(|(query, _)| query.as_deref() != Some(id));
            pending.delivery.send(QueryEvent::Terminated, true);
            tracing::debug!(
                query_id = id,
                elapsed_ms = pending.started.elapsed().as_millis(),
                "query terminated"
            );
            if self.ready && pending.submitted {
                let action_id = self.next_id();
                self.queue_action(Action::Terminate {
                    id: action_id,
                    terminate_id: pending.wire_id,
                });
            }
        }
    }

    fn queue_action(&mut self, action: Action) {
        // The action contains strings only, so JSON serialization is infallible.
        if let Ok(line) = serde_json::to_string(&action) {
            self.outgoing.push_back((None, line));
        }
    }

    fn submit_pending(&mut self, stable_id: &str) {
        let Some(pending) = self.pending.get_mut(stable_id) else {
            return;
        };
        let query = pending
            .request
            .query(pending.wire_id.clone(), &pending.remaining);
        match serde_json::to_string(&Request::Analysis(Box::new(query))) {
            Ok(line) => {
                pending.submitted = true;
                self.outgoing.push_back((Some(stable_id.into()), line));
            }
            Err(error) => {
                pending.delivery.send(
                    QueryEvent::Failed(QueryError::Engine(error.to_string())),
                    true,
                );
                self.pending.remove(stable_id);
            }
        }
    }

    fn output(&mut self, output: transport::Output, accept_readiness: bool) -> Option<Outcome> {
        self.last_output = Instant::now();
        let line = match output {
            transport::Output::Stdout(line) => line,
            transport::Output::Stderr(line) => {
                tracing::debug!(stderr = %line, "engine stderr");
                return None;
            }
            transport::Output::Fault(reason) => {
                return Some(Outcome::Fault {
                    reason,
                    fatal: !self.ready,
                });
            }
        };
        let parsed = match parse_line(&line) {
            Ok(parsed) => {
                self.bad_lines = 0;
                parsed
            }
            Err(error) => {
                self.bad_lines = self.bad_lines.saturating_add(1);
                self.total_bad_lines = self.total_bad_lines.saturating_add(1);
                tracing::warn!(consecutive = self.bad_lines, total = self.total_bad_lines, %error, "invalid engine output");
                return (self.bad_lines > self.cfg.max_consecutive_bad_lines).then(|| {
                    Outcome::Fault {
                        reason: format!("too many consecutive malformed lines: {}", self.bad_lines),
                        fatal: false,
                    }
                });
            }
        };
        match parsed {
            EngineLine::Action(ActionResponse::QueryVersion { id, version, .. })
                if accept_readiness && !self.ready && id == self.version_id =>
            {
                self.ready = true;
                let mut ids: Vec<_> = self
                    .pending
                    .iter()
                    .map(|(id, p)| (p.ordinal, id.clone()))
                    .collect();
                ids.sort_by_key(|(ordinal, _)| *ordinal);
                for (_, id) in ids {
                    let replay = self.pending.get(&id).is_some_and(|p| p.submitted);
                    if replay {
                        let new_id = self.next_id();
                        if let Some(pending) = self.pending.get_mut(&id) {
                            tracing::debug!(query_id = %id, wire_id = %new_id, "replaying query");
                            pending.wire_id = new_id;
                            pending.delivery.send(QueryEvent::Restarted, false);
                        }
                    }
                    self.submit_pending(&id);
                }
                self.status.send_replace(EngineStatus::Ready { version });
            }
            EngineLine::Analysis(response) => {
                if let Some(id) = self.stable_id(&response.id) {
                    let pending = self.pending.get_mut(&id)?;
                    if !pending.remaining.contains(&response.turn_number) {
                        return None;
                    }
                    if response.is_during_search {
                        pending.delivery.send(QueryEvent::Partial(*response), false);
                    } else {
                        pending.remaining.remove(&response.turn_number);
                        let complete = pending.remaining.is_empty();
                        tracing::debug!(query_id = %id, wire_id = %response.id, elapsed_ms = pending.started.elapsed().as_millis(), "query final");
                        pending
                            .delivery
                            .send(QueryEvent::Final(*response), complete);
                        if complete {
                            self.pending.remove(&id);
                        }
                    }
                }
            }
            EngineLine::NoResults(response) => {
                if let Some(id) = self.stable_id(&response.id)
                    && let Some(pending) = self.pending.remove(&id)
                {
                    pending.delivery.send(
                        QueryEvent::Failed(QueryError::NoResults(response.turn_number)),
                        true,
                    );
                }
            }
            EngineLine::Error(error) => {
                if let Some(wire_id) = error.id {
                    if let Some(id) = self.stable_id(&wire_id)
                        && let Some(pending) = self.pending.remove(&id)
                    {
                        pending
                            .delivery
                            .send(QueryEvent::Failed(QueryError::Engine(error.error)), true);
                    } else if wire_id == self.version_id {
                        return Some(Outcome::Fault {
                            reason: error.error,
                            fatal: true,
                        });
                    }
                } else {
                    return Some(Outcome::Fault {
                        reason: error.error,
                        fatal: !self.ready,
                    });
                }
            }
            EngineLine::Warning(warning) => {
                tracing::warn!(query_id = ?warning.id, message = %warning.warning, "engine warning");
            }
            EngineLine::Action(_) => {}
        }
        None
    }

    fn stable_id(&self, wire_id: &str) -> Option<QueryId> {
        self.pending
            .iter()
            .find_map(|(id, pending)| (pending.wire_id == wire_id).then(|| id.clone()))
    }

    fn fail_queries(&mut self, error: QueryError) {
        for (_, pending) in std::mem::take(&mut self.pending) {
            pending
                .delivery
                .send(QueryEvent::Failed(error.clone()), true);
        }
    }

    fn fail(&mut self, reason: String, stderr_tail: Vec<String>) {
        self.ready = false;
        tracing::error!(%reason, "engine session failed");
        self.fail_queries(QueryError::Session(reason.clone()));
        self.status.send_replace(EngineStatus::Failed {
            reason,
            stderr_tail,
        });
    }

    async fn wait_failed(&mut self) {
        while !self.stopping {
            let command = self.commands.recv().await;
            self.command(command);
        }
    }
    /// Keep polling the same driver and apply already-emitted results before
    /// deciding which queries need replay. Readiness cannot restart I/O here.
    async fn finish_transport(&mut self, process: &mut transport::Transport) -> transport::Exit {
        // drive may already have polled the driver to completion before consuming
        // a queued I/O fault. Never poll that completed future a second time.
        if let Ok(exit) = process.exited.try_recv() {
            while let Ok(output) = process.incoming.try_recv() {
                self.output(output, false);
            }
            return exit;
        }
        if let Some(stop) = process.stop.take() {
            let _ = stop.send(());
        }
        let mut done = false;
        let mut open = true;
        loop {
            tokio::select! {
                _ = &mut process.driver, if !done => done = true,
                output = process.incoming.recv(), if open => {
                    match output {
                        Some(output) => { self.output(output, false); }
                        None => open = false,
                    }
                }
                exit = &mut process.exited => {
                    while let Ok(output) = process.incoming.try_recv() { self.output(output, false); }
                    return exit.unwrap_or_else(|_| transport::Exit {
                        status: Err("transport lost exit report".into()), stderr_tail: Vec::new(),
                    });
                }
            }
        }
    }
}
