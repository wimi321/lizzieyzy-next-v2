//! One command path for both live coordinators. Results and caches stay separate.
use super::Actor;
use crate::AppError;
use analysis::{AnalysisHandle, Focus};
use katago_protocol::AnalysisResponse;

#[derive(Clone)]
pub(super) enum Update {
    Focus(Box<Focus>, Box<[Option<AnalysisResponse>; 2]>, bool),
    Clear,
    Pondering(bool),
    Resume,
    Limits(analysis::AnalysisLimits),
    Live(analysis::LiveSettings),
    Extras(analysis::AnalysisExtras),
    Pure(bool),
    Restriction(Option<analysis::AnalysisRestriction>),
    Cache(bool),
    Discard,
}

impl Update {
    async fn apply(
        &self,
        handle: &AnalysisHandle,
        slot: usize,
    ) -> Result<(), analysis::AnalysisError> {
        match self {
            Self::Focus(focus, cached, resume) => {
                handle
                    .focus_with_cached_and_resume(*focus.clone(), cached[slot].clone(), *resume)
                    .await
            }
            Self::Clear => handle.clear_focus().await,
            Self::Pondering(enabled) => handle.set_pondering(*enabled).await,
            Self::Resume => handle.resume().await,
            Self::Limits(limits) => handle.set_limits(limits.clone()).await,
            Self::Live(live) => handle.set_live_settings(live.clone()).await,
            Self::Extras(extras) => handle.set_analysis_extras(*extras).await,
            Self::Pure(enabled) => handle.set_pure_network(*enabled).await,
            Self::Restriction(restriction) => handle.set_restriction(restriction.clone()).await,
            Self::Cache(enabled) => handle.set_cache_enabled(*enabled).await,
            Self::Discard => handle.discard_cached_analysis().await,
        }
    }
}

impl Actor {
    /// Always attempt both deliveries. A comparison failure must never turn a
    /// successful main-engine/document command into a failure.
    pub(super) async fn update_coordinators(&mut self, update: Update) -> Result<(), AppError> {
        let main = match &self.running {
            Some(running) => update.apply(&running.coordinator, 0).await,
            None => Ok(()),
        };
        if let Some(running) = &self.comparison.running
            && let Err(error) = update.apply(&running.coordinator, 1).await
        {
            self.comparison_failed(error.to_string());
        }
        if matches!(
            update,
            Update::Clear
                | Update::Discard
                | Update::Pure(_)
                | Update::Restriction(_)
                | Update::Live(_)
                | Update::Extras(_)
        ) {
            self.comparison.latest = None;
            self.output.clear_comparison_analysis();
        }
        main.map_err(Into::into)
    }
}
