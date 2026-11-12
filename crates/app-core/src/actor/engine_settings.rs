use super::Actor;
use crate::{AppError, EngineSettings};
use std::time::Duration;

impl Actor {
    pub(super) fn session_config(&self) -> engine_session::SessionConfig {
        let mut config = self.config.session.clone();
        config.auto_check_alive = self.settings.engine_settings.auto_check_alive;
        config
    }

    pub(super) fn live_settings(&self) -> analysis::LiveSettings {
        let mut live = self.config.analysis.live.clone();
        let settings = &self.settings.engine_settings;
        // Explicit host tuning remains useful in headless callers/tests. Saved
        // nondefault user cadence always takes precedence over host tuning.
        if settings.report_interval_ms != EngineSettings::default().report_interval_ms
            || live.report_interval == analysis::LiveSettings::default().report_interval
        {
            live.report_interval = Duration::from_millis(u64::from(settings.report_interval_ms));
        }
        live.query_rules = settings.rules.query_rules();
        live.query_komi = (!settings.read_komi).then_some(settings.default_komi);
        live
    }

    pub(super) fn evaluation_fingerprint(&self, profile: &str) -> Result<String, AppError> {
        let mut hash = blake3::Hasher::new();
        hash.update(profile.as_bytes());
        hash.update(&self.settings.engine_settings.evaluation_identity()?);
        Ok(hash.finalize().to_hex().to_string())
    }

    pub(super) fn can_reuse_saved_analysis(&self) -> bool {
        self.settings.engine_settings.rules == crate::EngineRules::FollowSgf
            && self.settings.engine_settings.read_komi
    }

    pub(super) async fn set_engine_settings(
        &mut self,
        value: EngineSettings,
    ) -> Result<EngineSettings, AppError> {
        self.ensure_no_benchmark()?;
        value.validate()?;
        let previous = self.settings.engine_settings.clone();
        if value == previous {
            return Ok(value);
        }
        let mut settings = self.settings.clone();
        settings.engine_settings = value.clone();
        settings.save(&self.directory).await?;
        self.settings = settings;
        self.engine_settings_changed(&previous).await?;
        Ok(value)
    }

    /// Apply an already saved import/setter transaction without saving twice.
    pub(super) async fn engine_settings_changed(
        &mut self,
        previous: &EngineSettings,
    ) -> Result<(), AppError> {
        if previous == &self.settings.engine_settings {
            return Ok(());
        }
        self.auto_setup.cancel();
        self.cancel_trial().await;
        let evaluation_changed = previous.evaluation_identity()?
            != self.settings.engine_settings.evaluation_identity()?;
        let resume_review = self.review_running();
        if evaluation_changed {
            self.flush_cached_analysis().await;
            self.cancel_review().await;
            // Saved LZ values keep their explicit file origin; they never seed
            // the engine cache. Fresh engine queries replace them normally.
            self.set_summary_fingerprint(None);
            self.focus = None;
            self.output.clear_analysis();
        } else if resume_review {
            self.stop_review_query().await?;
        }
        if let Some(running) = &self.running {
            running
                .engine
                .set_auto_check_alive(self.settings.engine_settings.auto_check_alive)
                .await?;
        }
        if evaluation_changed {
            if let Some(mut running) = self.running.take() {
                let replacement = self.coordinator(&running.engine, &running.fingerprint);
                match replacement {
                    Ok((coordinator, snapshots)) => {
                        let previous = std::mem::replace(&mut running.coordinator, coordinator);
                        running.analysis_status = running.coordinator.status();
                        running.snapshots = snapshots;
                        self.set_summary_fingerprint(Some(running.fingerprint.clone()));
                        self.running = Some(running);
                        // The replacement is retained even if old cleanup fails.
                        if let Err(error) = previous.shutdown().await {
                            self.analysis_failed(error.to_string());
                        }
                    }
                    Err(error) => {
                        self.running = Some(running);
                        self.analysis_failed(error.to_string());
                        return Err(error);
                    }
                }
            }
        } else if let Some(running) = &self.running {
            running
                .coordinator
                .set_live_settings(self.live_settings())
                .await?;
        }
        self.update_focus().await;
        if !evaluation_changed && resume_review {
            self.continue_review().await;
        }
        self.publish_document()?;
        Ok(())
    }
}
