use super::{Actor, trial::Reply};
use crate::{
    AppError, EngineProfile, EngineSetupError,
    auto_setup::{Start, Ticket, Trial},
};
use tokio::sync::{oneshot, watch};

#[derive(Default)]
pub(super) struct State {
    generation: u64,
    cancellation: Option<watch::Sender<()>>,
}

impl State {
    pub(super) fn cancel(&mut self) {
        self.cancellation = None;
    }
}

impl Actor {
    pub(super) async fn begin_auto_setup(&mut self) -> Result<Start, AppError> {
        if let Some(profile) = &self.settings.engine {
            return Ok(Start::Existing(profile.clone()));
        }
        self.auto_setup.cancel();
        self.cancel_trial().await;
        self.auto_setup.generation = self
            .auto_setup
            .generation
            .checked_add(1)
            .ok_or(AppError::RevisionLimit)?;
        let (sender, cancelled) = watch::channel(());
        self.auto_setup.cancellation = Some(sender);
        Ok(Start::Pending(Ticket {
            generation: self.auto_setup.generation,
            cancelled,
        }))
    }

    pub(super) async fn begin_auto_trial(
        &mut self,
        trial: Trial,
        reply: oneshot::Sender<Result<EngineProfile, AppError>>,
    ) {
        // Recheck inside the actor: discovery and version probes run outside it,
        // and a manual setting or another auto setup may have superseded them.
        if trial.generation != self.auto_setup.generation
            || self.auto_setup.cancellation.is_none()
            || self.settings.engine.is_some()
        {
            let _ = reply.send(Err(EngineSetupError::Cancelled.into()));
            return;
        }
        self.begin_trial_with_options(
            trial.profile,
            trial.source,
            Reply::Auto(reply),
            Some((trial.canonical_model_name, trial.deadline)),
        )
        .await;
    }
}
