use super::Actor;
use crate::{AnalysisExtras, AppError, WholeGameStatus};

impl Actor {
    pub(super) async fn set_analysis_extras(
        &mut self,
        extras: AnalysisExtras,
    ) -> Result<(), AppError> {
        let extras = extras.into();
        if self.config.analysis.extras == extras {
            return Ok(());
        }
        self.ensure_no_benchmark()?;
        // Reserve the live slot before replacing a query, just as navigation does.
        let restart_review = self.review_running();
        if restart_review {
            self.stop_review_query().await?;
        }
        self.output.clear_analysis();
        let result = if let Some(running) = &self.running {
            running
                .coordinator
                .set_analysis_extras(extras)
                .await
                .map_err(AppError::from)
        } else {
            Ok(())
        };
        if result.is_ok() {
            self.config.analysis.extras = extras;
        }
        if restart_review {
            self.continue_review().await;
        }
        result
    }

    pub(super) fn ensure_no_pure_network(&self) -> Result<(), AppError> {
        if self.config.analysis.pure_network {
            Err(AppError::PureNetworkActive)
        } else {
            Ok(())
        }
    }

    pub(super) async fn set_pure_network(&mut self, enabled: bool) -> Result<(), AppError> {
        if enabled == self.config.analysis.pure_network {
            return Ok(());
        }
        self.ensure_no_benchmark()?;
        if enabled {
            self.ensure_no_batch()?;
            if self.review.progress.as_ref().is_some_and(|progress| {
                matches!(
                    progress.status,
                    WholeGameStatus::Running | WholeGameStatus::Paused
                )
            }) {
                return Err(AppError::WholeGameAnalysisActive);
            }
        }
        self.output.clear_analysis();
        if let Some(running) = &self.running {
            running.coordinator.set_pure_network(enabled).await?;
        }
        self.config.analysis.pure_network = enabled;
        // Pure network follows the displayed board, including local browsing
        // during board sync. The normal sync focus is restored on exit.
        self.update_focus().await;
        if !enabled {
            self.publish_saved_focus();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
