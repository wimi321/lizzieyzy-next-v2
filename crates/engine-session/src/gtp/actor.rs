use super::{GtpDirection, GtpInfo, GtpLogLine, GtpSessionConfig, GtpSessionError, GtpStatus};
use crate::{
    SessionConfig, TransportSpec,
    transport::{self, Output, Transport},
};
use std::{collections::VecDeque, future::pending, time::Duration};
use tokio::{
    sync::{broadcast, mpsc, oneshot, watch},
    time::{Instant, sleep_until, timeout},
};

pub(super) enum Message {
    Execute(
        String,
        Option<Duration>,
        oneshot::Sender<Result<::gtp::Response, GtpSessionError>>,
    ),
    Console(oneshot::Sender<Vec<GtpLogLine>>),
    Shutdown(oneshot::Sender<Result<(), GtpSessionError>>),
}

type Reply = oneshot::Sender<Result<::gtp::Response, GtpSessionError>>;
struct Pending {
    text: String,
    reply: Option<Reply>,
    startup: bool,
    timeout: Option<Duration>,
}
struct Active {
    pending: Pending,
    id: Option<u32>,
    deadline: Instant,
}
struct Actor {
    config: GtpSessionConfig,
    status: watch::Sender<GtpStatus>,
    lines: broadcast::Sender<GtpLogLine>,
    history: VecDeque<GtpLogLine>,
    sequence: u64,
    startup: VecDeque<String>,
    queued: VecDeque<Pending>,
    active: Option<Active>,
    decoder: ::gtp::Decoder,
    info: GtpInfo,
    failure: Option<GtpSessionError>,
}

pub(super) async fn run(
    spec: TransportSpec,
    config: GtpSessionConfig,
    mut messages: mpsc::Receiver<Message>,
    status: watch::Sender<GtpStatus>,
    lines: broadcast::Sender<GtpLogLine>,
    final_history: watch::Sender<Vec<GtpLogLine>>,
) {
    let mut actor = Actor {
        startup: ["protocol_version", "name", "version", "list_commands"]
            .into_iter()
            .map(str::to_owned)
            .chain(config.init_commands.clone())
            .collect(),
        config,
        status,
        lines,
        history: VecDeque::new(),
        sequence: 0,
        queued: VecDeque::new(),
        active: None,
        decoder: ::gtp::Decoder::default(),
        info: GtpInfo::default(),
        failure: None,
    };
    let cfg = SessionConfig {
        write_timeout: actor.config.write_timeout,
        shutdown_timeout: actor.config.shutdown_timeout,
        ..SessionConfig::default()
    };
    let validation = validate(&actor.config);
    let mut transport = match validation {
        Ok(()) => {
            match timeout(actor.config.startup_timeout, transport::start(&spec, &cfg)).await {
                Ok(Ok(transport)) => Some(transport),
                Ok(Err(error)) => {
                    actor.fail(GtpSessionError::Process(error.to_string()));
                    None
                }
                Err(_) => {
                    actor.fail(GtpSessionError::Timeout {
                        command: "spawn".into(),
                    });
                    None
                }
            }
        }
        Err(error) => {
            actor.fail(error);
            None
        }
    };
    let mut shutdown = None;
    if let Some(io) = transport.as_mut() {
        loop {
            if actor.failure.is_some() {
                break;
            }
            if let Err(error) = actor.start_next(io) {
                actor.fail(error);
                break;
            }
            let deadline = actor.active.as_ref().map(|active| active.deadline);
            tokio::select! {
                // Fair polling keeps control and deadlines responsive to noisy engines.
                output = io.incoming.recv() => match output {
                    Some(output) => actor.output(output),
                    None => { actor.fail(GtpSessionError::Process("stdout/stderr closed".into())); break; }
                },
                _ = &mut io.driver => {
                    while let Ok(output) = io.incoming.try_recv() { actor.output(output); }
                    let detail = match io.exited.try_recv() {
                        Ok(exit) => match exit.status { Ok(status) => status.to_string(), Err(error) => error },
                        Err(error) => error.to_string(),
                    };
                    actor.fail(GtpSessionError::Process(detail));
                    transport = None;
                    break;
                },
                message = messages.recv() => match message {
                    Some(Message::Shutdown(reply)) => { shutdown = Some(reply); break; }
                    Some(message) => actor.message(message),
                    None => break,
                },
                _ = deadline_wait(deadline) => {
                    let command = actor.active.as_ref().map(|active| active.pending.text.clone()).unwrap_or_default();
                    actor.fail(GtpSessionError::Timeout { command });
                },
            }
        }
    }
    actor.cancel_pending();
    let cleanup = if let Some(io) = transport.as_mut() {
        actor.cleanup(io).await
    } else {
        Ok(())
    };
    final_history.send_replace(actor.history.iter().cloned().collect());
    if let Some(reply) = shutdown {
        actor.status.send_replace(GtpStatus::Stopped);
        let _ = reply.send(cleanup);
        return;
    }
    // Retain failed-session console history until the owner shuts it down.
    while let Some(message) = messages.recv().await {
        match message {
            Message::Shutdown(reply) => {
                actor.status.send_replace(GtpStatus::Stopped);
                let _ = reply.send(cleanup);
                return;
            }
            other => actor.message(other),
        }
    }
    actor.status.send_replace(GtpStatus::Stopped);
}

fn validate(config: &GtpSessionConfig) -> Result<(), GtpSessionError> {
    if config.queue_capacity == 0
        || config.queue_capacity > 1024
        || config.init_commands.len() > 256
        || [
            config.startup_timeout,
            config.response_timeout,
            config.write_timeout,
            config.shutdown_timeout,
        ]
        .iter()
        .any(|d| d.is_zero() || *d > std::time::Duration::from_secs(86400))
    {
        return Err(GtpSessionError::InvalidConfig(
            "positive timeouts <=24h, queue 1..=1024, at most 256 init commands required".into(),
        ));
    }
    for command in &config.init_commands {
        ::gtp::Command::parse(command)
            .map_err(|error| GtpSessionError::InvalidConfig(error.to_string()))?;
    }
    Ok(())
}

async fn deadline_wait(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => pending().await,
    }
}

impl Actor {
    fn message(&mut self, message: Message) {
        match message {
            Message::Console(reply) => {
                let _ = reply.send(self.history.iter().cloned().collect());
            }
            Message::Execute(text, timeout, reply) => {
                if let Some(error) = &self.failure {
                    let _ = reply.send(Err(error.clone()));
                } else if self.queued.len() >= self.config.queue_capacity {
                    let _ = reply.send(Err(GtpSessionError::Capacity));
                } else {
                    self.queued.push_back(Pending {
                        text,
                        reply: Some(reply),
                        startup: false,
                        timeout,
                    });
                }
            }
            Message::Shutdown(reply) => {
                let _ = reply.send(Err(GtpSessionError::Closed));
            }
        }
    }

    fn start_next(&mut self, io: &Transport) -> Result<(), GtpSessionError> {
        if self.active.is_some() {
            return Ok(());
        }
        let pending = if let Some(text) = self.startup.pop_front() {
            Some(Pending {
                text,
                reply: None,
                startup: true,
                timeout: None,
            })
        } else {
            if matches!(*self.status.borrow(), GtpStatus::Starting) {
                self.status
                    .send_replace(GtpStatus::Ready(self.info.clone()));
            }
            self.queued.pop_front()
        };
        if let Some(pending) = pending {
            let id = ::gtp::Command::parse(&pending.text)
                .map_err(|error| GtpSessionError::Protocol(error.to_string()))?
                .id;
            io.outgoing
                .try_send(pending.text.clone())
                .map_err(|error| GtpSessionError::Process(error.to_string()))?;
            self.log(GtpDirection::Sent, pending.text.clone());
            let budget = if pending.startup {
                self.config.startup_timeout
            } else {
                pending.timeout.unwrap_or(self.config.response_timeout)
            };
            let deadline = Instant::now().checked_add(budget).ok_or_else(|| {
                GtpSessionError::InvalidConfig("command timeout is not representable".into())
            })?;
            self.active = Some(Active {
                pending,
                id,
                deadline,
            });
        }
        Ok(())
    }

    fn log(&mut self, direction: GtpDirection, mut text: String) {
        // Preserve line ordering while bounding diagnostics even for noisy stderr.
        if text.len() > ::gtp::MAX_RESPONSE_BYTES {
            let mut end = ::gtp::MAX_RESPONSE_BYTES;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
        }
        self.sequence = self.sequence.saturating_add(1);
        let line = GtpLogLine {
            sequence: self.sequence,
            direction,
            text,
        };
        if self.history.len() == 2000 {
            self.history.pop_front();
        }
        self.history.push_back(line.clone());
        let _ = self.lines.send(line);
    }

    fn output(&mut self, output: Output) {
        match output {
            Output::Stderr(line) => self.log(GtpDirection::Stderr, line),
            Output::Fault(error) => self.fail(GtpSessionError::Process(error)),
            Output::Stdout(line) => {
                self.log(GtpDirection::Received, line.clone());
                if self.failure.is_some() {
                    return;
                }
                match self.decoder.push_line(&line) {
                    Ok(Some(response)) => self.response(response),
                    Ok(None) => {}
                    Err(error) => self.fail(GtpSessionError::Protocol(error.to_string())),
                }
            }
        }
    }

    fn response(&mut self, response: ::gtp::Response) {
        let Some(active) = self.active.take() else {
            self.fail(GtpSessionError::Protocol("unsolicited response".into()));
            return;
        };
        let Pending {
            text,
            reply,
            startup,
            ..
        } = active.pending;
        let result = if active.id != response.id {
            Err(GtpSessionError::Protocol(
                "response id does not match command".into(),
            ))
        } else if !response.success {
            Err(GtpSessionError::Rejected {
                command: text.clone(),
                message: response.body.clone(),
            })
        } else {
            Ok(response)
        };
        if startup {
            match result {
                Ok(response) => match text.as_str() {
                    "protocol_version" if response.body.trim() != "2" => {
                        self.fail(GtpSessionError::Startup {
                            command: text,
                            message: "only GTP 2 is supported".into(),
                        });
                    }
                    "protocol_version" => self.info.protocol_version = response.body.trim().into(),
                    "name" => self.info.name = response.body,
                    "version" => self.info.version = response.body,
                    "list_commands" => {
                        self.info.commands = response
                            .body
                            .split_ascii_whitespace()
                            .map(str::to_owned)
                            .collect()
                    }
                    _ => {}
                },
                Err(error) => self.fail(GtpSessionError::Startup {
                    command: text,
                    message: error.to_string(),
                }),
            }
        } else {
            let fatal = match &result {
                Err(error @ GtpSessionError::Protocol(_)) => Some(error.clone()),
                _ => None,
            };
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
            if let Some(error) = fatal {
                self.fail(error);
            }
        }
    }

    fn fail(&mut self, error: GtpSessionError) {
        if self.failure.is_some() {
            return;
        }
        self.status.send_replace(GtpStatus::Failed(error.clone()));
        self.failure = Some(error);
        self.cancel_pending();
    }

    fn cancel_pending(&mut self) {
        let error = self.failure.clone().unwrap_or(GtpSessionError::Closed);
        if let Some(active) = self.active.take()
            && let Some(reply) = active.pending.reply
        {
            let _ = reply.send(Err(error.clone()));
        }
        for pending in self.queued.drain(..) {
            if let Some(reply) = pending.reply {
                let _ = reply.send(Err(error.clone()));
            }
        }
    }

    async fn cleanup(&mut self, io: &mut Transport) -> Result<(), GtpSessionError> {
        if let Some(stop) = io.stop.take() {
            let _ = stop.send(());
        }
        loop {
            tokio::select! {
                _ = &mut io.driver => break,
                output = io.incoming.recv() => if let Some(output) = output { self.output(output); },
            }
        }
        while let Ok(output) = io.incoming.try_recv() {
            self.output(output);
        }
        match (&mut io.exited).await {
            Ok(exit) => exit.status.map(|_| ()).map_err(GtpSessionError::Process),
            Err(error) => Err(GtpSessionError::Process(error.to_string())),
        }
    }
}
