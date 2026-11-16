use super::{Actor, Trial};
use crate::{
    AppError, WebClientMessage as ClientMessage, WebServerMessage as Message,
    WebTrialDeniedReason as Denied,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Instant;

impl Actor {
    pub(in crate::actor) fn web_deadline(&self) -> Option<Instant> {
        let trial = self.web_board.trial.as_ref()?;
        let (idle, reconnect) = self.web_timeouts();
        let idle = trial.last_activity + idle;
        Some(
            trial
                .disconnected
                .map_or(idle, |time| idle.min(time + reconnect)),
        )
    }

    pub(in crate::actor) async fn expire_web_trial(&mut self) {
        if self
            .web_deadline()
            .is_some_and(|deadline| deadline <= Instant::now())
        {
            let _ = self.end_web_trial().await;
        }
    }

    fn web_timeouts(&self) -> (Duration, Duration) {
        #[cfg(test)]
        if let Some(config) = &self.config.web_board {
            return (config.idle_timeout, config.reconnect_timeout);
        }
        (Duration::from_secs(300), Duration::from_secs(30))
    }

    pub(super) async fn end_web_trial(&mut self) -> Result<(), AppError> {
        if self.web_board.trial.is_none() {
            return Ok(());
        }
        self.end_move_trial().await?;
        self.web_board.trial = None;
        self.refresh_web_board();
        Ok(())
    }

    pub(super) async fn web_client_message(&mut self, connection: u64, message: ClientMessage) {
        let Some(client) = self.web_board.clients.get_mut(&connection) else {
            return;
        };
        let now = Instant::now();
        while client
            .messages
            .front()
            .is_some_and(|time| now.duration_since(*time) >= Duration::from_secs(1))
        {
            client.messages.pop_front();
        }
        if client.messages.len() >= 10 {
            return;
        }
        client.messages.push_back(now);
        let owner = client.id.clone();
        if message == ClientMessage::EnterTrial {
            let denied = if !self.settings.web_board.allow_trial {
                Some(Denied::Disabled)
            } else if self
                .web_board
                .trial
                .as_ref()
                .is_some_and(|trial| trial.owner != owner)
            {
                Some(Denied::InUse)
            } else if self.web_board.trial.is_none() && self.web_host_busy() {
                Some(Denied::HostBusy)
            } else {
                None
            };
            if let Some(reason) = denied {
                self.web_reply(connection, Message::TrialDenied { reason });
                return;
            }
            if self.web_board.trial.is_none() {
                let anchor = self.document.current();
                let anchor_move = self
                    .tree
                    .nodes
                    .iter()
                    .find(|node| node.id == anchor.0)
                    .map_or(0, |node| node.move_number);
                if let Err(error) = self.start_move_trial().await {
                    self.web_reply(
                        connection,
                        Message::TrialRejected {
                            reason: error.to_string(),
                        },
                    );
                    return;
                }
                let Some(client) = self.web_board.clients.get(&connection) else {
                    return;
                };
                self.web_board.trial = Some(Trial {
                    owner,
                    reconnect: client.reconnect.clone(),
                    since: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |value| {
                            value.as_millis().min(u128::from(u64::MAX)) as u64
                        }),
                    anchor,
                    anchor_move,
                    last_activity: now,
                    disconnected: None,
                    messages: client.messages.clone(),
                });
            }
            return;
        }
        if self
            .web_board
            .trial
            .as_ref()
            .is_none_or(|trial| trial.owner != owner)
        {
            self.web_reply(
                connection,
                Message::TrialDenied {
                    reason: if self.settings.web_board.allow_trial {
                        Denied::InUse
                    } else {
                        Denied::Disabled
                    },
                },
            );
            return;
        }
        let result = self.web_trial_action(message).await;
        if let Err(error) = result {
            self.web_reply(
                connection,
                Message::TrialRejected {
                    reason: error.to_string(),
                },
            );
        } else if let Some(trial) = &mut self.web_board.trial {
            trial.last_activity = now;
        }
    }

    async fn web_trial_action(&mut self, message: ClientMessage) -> Result<(), AppError> {
        let Some(trial) = &self.web_board.trial else {
            return Ok(());
        };
        let anchor = trial.anchor;
        let command = match message {
            ClientMessage::ExitTrial => return self.end_web_trial().await,
            ClientMessage::TrialMove { x, y } => {
                let color = self
                    .document
                    .position_at(self.document.current())?
                    .board
                    .to_play();
                document::Command::Play(go_core::Move::Play(color, go_core::Point { x, y }))
            }
            ClientMessage::TrialBack if self.document.current() != anchor => {
                document::Command::Step(-1)
            }
            ClientMessage::TrialForward => document::Command::Step(1),
            ClientMessage::TrialToAnchor => document::Command::GoTo(anchor),
            ClientMessage::TrialChild { index } => {
                let child = self
                    .document
                    .node(self.document.current())?
                    .children()
                    .get(index)
                    .copied()
                    .ok_or(crate::WebBoardError::InvalidBranch)?;
                document::Command::GoTo(child)
            }
            ClientMessage::TrialBack | ClientMessage::EnterTrial => return Ok(()),
        };
        self.apply(super::super::Input::Edit(command))
            .await
            .map(|_| ())
    }

    fn web_reply(&self, connection: u64, message: Message) {
        if let Some(client) = self.web_board.clients.get(&connection) {
            client.send(message);
        }
    }

    fn web_host_busy(&self) -> bool {
        self.ensure_research_available().is_err()
            || self.move_trial.is_some()
            || self.tsumego.is_some()
            || self.scoring.is_some()
            || self.trial.is_some()
            || self.acceleration.backend.is_some()
            || self.sync_active()
            || self.config.analysis.pure_network
            || self.restriction.is_some()
            || self.review.progress.as_ref().is_some_and(|p| {
                matches!(
                    p.status,
                    crate::WholeGameStatus::Running | crate::WholeGameStatus::Paused
                )
            })
    }
}
