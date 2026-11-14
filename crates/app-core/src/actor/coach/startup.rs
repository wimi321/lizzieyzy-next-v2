use super::*;
use engine_session::EngineStatus;

pub(super) async fn receive(starting: &mut Starting) -> Result<(), AppError> {
    if let Some(error) = starting.error.take() {
        return Err(error);
    }
    let status = starting.status.as_mut().ok_or(CoachError::NoEngine)?;
    loop {
        match status.borrow_and_update().clone() {
            EngineStatus::Ready { .. } => return Ok(()),
            EngineStatus::Failed {
                reason,
                stderr_tail,
            } => {
                let reason = if stderr_tail.is_empty() {
                    reason
                } else {
                    format!("{reason}; stderr: {}", stderr_tail.join("\n"))
                };
                return Err(crate::GameError::Engine(reason).into());
            }
            EngineStatus::Stopped => {
                return Err(crate::GameError::Engine("engine stopped before Ready".into()).into());
            }
            _ => {}
        }
        tokio::select! {
            changed = status.changed() => if changed.is_err() { return Err(CoachError::NoEngine.into()); },
            () = tokio::time::sleep_until(starting.deadline) => return Err(crate::GameError::Engine("HumanSL engine startup timed out".into()).into()),
            // A dropped caller still requires recovery of the ordinary engine.
            () = starting.reply.closed(), if matches!(starting.phase, StartupPhase::Loading { .. }) => return Err(AppError::Closed),
        }
    }
}

impl Actor {
    pub(super) async fn begin_coach_start(
        &mut self,
        prepared: Prepared,
        reply: oneshot::Sender<Result<CoachView, AppError>>,
    ) {
        self.coach.preparing = true;
        let error = match prepared.restart {
            Some(profile) => self.configure_engine(Some(profile)).await.err(),
            None => None,
        };
        self.coach.starting = Some(Starting {
            setup: prepared.setup,
            reply,
            status: self.running.as_ref().map(|r| r.engine.status()),
            deadline: Instant::now() + self.config.engine_setup_timeout,
            phase: StartupPhase::Loading {
                restore: prepared.restore.map(Box::new),
            },
            error,
        });
        self.coach.preparing = false;
    }

    pub(super) async fn finish_coach_start(&mut self, result: Result<(), AppError>) {
        let Some(mut starting) = self.coach.starting.take() else {
            return;
        };
        let result = match (starting.phase, result) {
            (
                StartupPhase::Loading {
                    restore: Some(profile),
                },
                Err(error),
            ) => {
                let reason = error.to_string();
                // configure_engine restores both the selected list entry and the
                // legacy profile, persists them, then restarts the sole owner.
                self.coach.preparing = true;
                let error = self.configure_engine(Some(*profile)).await.err();
                self.coach.preparing = false;
                starting.phase = StartupPhase::Restoring { reason };
                starting.error = error;
                starting.status = self.running.as_ref().map(|r| r.engine.status());
                starting.deadline = Instant::now() + self.config.engine_setup_timeout;
                self.coach.starting = Some(starting);
                return;
            }
            (StartupPhase::Loading { .. }, Ok(())) => self.activate_coach(starting.setup).await,
            (StartupPhase::Loading { .. }, Err(error)) => Err(error),
            (StartupPhase::Restoring { mut reason }, restored) => {
                if let Err(error) = restored {
                    reason.push_str(&format!("; restoring the original engine failed: {error}"));
                }
                Err(CoachError::HumanModelLoadFailed { reason }.into())
            }
        };
        if result.is_err() {
            self.update_focus().await;
        }
        let _ = starting.reply.send(result);
    }
}
