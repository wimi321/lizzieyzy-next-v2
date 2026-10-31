//! Bounded first-run setup. Discovery and version I/O never occupy the app actor;
//! only the existing trial transaction may publish a successful configuration.
use crate::{
    AppCore, AppError, EngineProfile, EngineSetupProfile,
    actor::Command,
    engine_setup::{self, SearchRoots},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use tokio::{
    sync::{oneshot, watch},
    time::Instant,
};
use ts_rs::TS;

mod version;

pub(crate) const MAX_ATTEMPTS: usize = 6;
pub(crate) const TOTAL_TIMEOUT: Duration = Duration::from_secs(120);
pub(crate) const TRIAL_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AutoSetupAttempt {
    pub program: PathBuf,
    pub model: PathBuf,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum AutoSetupError {
    #[error("no KataGo program was found")]
    NoPrograms,
    #[error("no model was found")]
    NoModels,
    #[error("automatic engine setup failed: {reason}; attempts: {attempts:?}")]
    AllFailed {
        attempts: Vec<AutoSetupAttempt>,
        reason: String,
    },
}

pub(crate) enum Start {
    Existing(EngineProfile),
    Pending(Ticket),
}

pub(crate) struct Ticket {
    pub generation: u64,
    pub cancelled: watch::Receiver<()>,
}

pub(crate) struct Trial {
    pub generation: u64,
    pub profile: EngineSetupProfile,
    pub source: PathBuf,
    pub canonical_model_name: Option<String>,
    pub deadline: Instant,
}

fn failed(attempts: Vec<AutoSetupAttempt>, reason: impl Into<String>) -> AppError {
    AutoSetupError::AllFailed {
        attempts,
        reason: reason.into(),
    }
    .into()
}

impl AppCore {
    /// Return saved settings untouched, otherwise discover and try a bounded
    /// sequence. The roots and template are explicit and require no host globals.
    pub async fn auto_configure_engine(
        &self,
        roots: SearchRoots,
        default_config_source: PathBuf,
    ) -> Result<EngineProfile, AppError> {
        self.auto_configure_using(
            move || engine_setup::detect_candidates(&roots),
            default_config_source,
        )
        .await
    }

    async fn auto_configure_using(
        &self,
        discover: impl FnOnce() -> engine_setup::DetectedEngines + Send + 'static,
        default_config_source: PathBuf,
    ) -> Result<EngineProfile, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::BeginAutoSetup(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        let start = response.await.map_err(|_| AppError::Closed)??;
        let mut ticket = match start {
            Start::Existing(profile) => return Ok(profile),
            Start::Pending(ticket) => ticket,
        };
        let deadline = Instant::now() + TOTAL_TIMEOUT;
        let discovery = tokio::task::spawn_blocking(discover);
        let detected = tokio::select! {
            _ = ticket.cancelled.changed() => return Err(failed(Vec::new(), "automatic setup was cancelled")),
            result = tokio::time::timeout_at(deadline, discovery) => match result {
                Ok(Ok(detected)) => detected,
                Ok(Err(error)) => return Err(failed(Vec::new(), format!("discovery failed: {error}"))),
                Err(_) => return Err(failed(Vec::new(), "automatic setup exceeded 120 seconds during discovery")),
            }
        };
        if detected.discovery.programs.is_empty() {
            return Err(AutoSetupError::NoPrograms.into());
        }
        if detected.discovery.models.is_empty() {
            return Err(AutoSetupError::NoModels.into());
        }
        let mut attempts = Vec::new();
        let mut versions: BTreeMap<PathBuf, Result<String, String>> = BTreeMap::new();
        for model in &detected.discovery.models {
            for program in detected.programs_for_model(model) {
                if attempts.len() >= MAX_ATTEMPTS {
                    return Err(failed(attempts, "automatic setup reached the 6-pair limit"));
                }
                if Instant::now() >= deadline {
                    return Err(failed(attempts, "automatic setup exceeded 120 seconds"));
                }
                let metadata = detected.catalog_models.get(model);
                let outcome = async {
                    let program_version = match versions.get(&program) {
                        Some(result) => result.clone(),
                        None => {
                            let result =
                                tokio::time::timeout_at(deadline, version::probe(&program))
                                    .await
                                    .unwrap_or_else(|_| {
                                        Err("automatic setup exceeded 120 seconds".into())
                                    });
                            versions.insert(program.clone(), result.clone());
                            result
                        }
                    }?;
                    if let Some(metadata) = metadata
                        && !version::supports(&program_version, &metadata.minimum_katago_version)
                    {
                        return Err(format!(
                            "KataGo {program_version} is below required {}",
                            metadata.minimum_katago_version
                        ));
                    }
                    let (reply, response) = oneshot::channel();
                    self.commands
                        .send(Command::TryAutoEngine(
                            Trial {
                                generation: ticket.generation,
                                profile: EngineSetupProfile {
                                    program: program.clone(),
                                    model: model.clone(),
                                    config: None,
                                    extra_args: Vec::new(),
                                },
                                source: default_config_source.clone(),
                                canonical_model_name: metadata.map(|model| model.file_name.clone()),
                                deadline,
                            },
                            reply,
                        ))
                        .await
                        .map_err(|_| AppError::Closed.to_string())?;
                    response
                        .await
                        .map_err(|_| AppError::Closed.to_string())?
                        .map_err(|error| error.to_string())
                };
                let result = tokio::select! {
                    // Once the actor starts the atomic settings commit, await
                    // its acknowledgement. A caller-side timeout could report
                    // failure after the file has already been published.
                    biased;
                    result = outcome => result,
                    _ = ticket.cancelled.changed() => return Err(failed(attempts, "automatic setup was cancelled")),
                };
                match result {
                    Ok(profile) => return Ok(profile),
                    Err(reason) => attempts.push(AutoSetupAttempt {
                        program,
                        model: model.clone(),
                        reason,
                    }),
                }
            }
        }
        Err(failed(attempts, "all candidate combinations failed"))
    }
}

#[cfg(test)]
mod tests;
