use super::coordinators::Update;
use super::*;

impl Actor {
    pub(super) async fn update_focus(&mut self) {
        self.update_focus_with_resume(false).await;
    }

    pub(super) async fn update_focus_with_resume(&mut self, resume_limited: bool) {
        if self.scoring.is_some() || self.benchmark_active() {
            return;
        }
        self.publish_saved_focus();
        self.publish_comparison_focus();
        if self.match_active() || self.game_owns_query() {
            return;
        }
        if self.running.is_none() && self.comparison.running.is_none() {
            return;
        }
        let node_id = if let Some(node) = self.game_focus_node() {
            node
        } else if self.sync_active() && !self.config.analysis.pure_network {
            self.board_sync
                .resolved_node
                .filter(|id| self.document.node(*id).is_ok())
                .unwrap_or(self.document.current())
        } else {
            self.document.current()
        };
        let Ok(position) = self.document.position_at(node_id) else {
            self.flush_cached_focus().await;
            self.focus = None;
            if let Err(error) = self.update_coordinators(Update::Clear).await {
                self.analysis_failed(error.to_string());
            }
            return;
        };
        let key = position.analysis_key();
        let changed = self
            .focus
            .as_ref()
            .is_none_or(|focus| focus.node_id != node_id.0 || focus.key != key);
        if changed {
            self.flush_cached_focus().await;
        }
        let restart_review = changed && self.review_running() && !self.zhizi_review_owns_engine();
        if restart_review && let Err(error) = self.stop_review_query().await {
            self.analysis_failed(error.to_string());
        }
        let view_focus = ViewFocus {
            revision: self.revision,
            node_id: node_id.0,
            key,
            size: position.board.size(),
            to_play: position.board.to_play(),
        };
        let cached = if self.restriction.is_some() {
            None
        } else {
            self.cached_focus(node_id, &position).await
        };
        let comparison_cached = self.comparison_cached_focus(node_id, &position).await;
        let focus = Focus {
            revision: self.revision,
            node_id,
            position,
            key,
        };
        if self.game_playing() {
            let _ = self
                .update_coordinators(Update::Pondering(self.pondering()))
                .await;
        }
        self.focus = Some(view_focus);
        match self
            .update_coordinators(Update::Focus(
                Box::new(focus),
                Box::new([cached, comparison_cached]),
                resume_limited,
            ))
            .await
        {
            Ok(()) => {
                if let Err(error) = self
                    .update_coordinators(Update::Pondering(self.pondering()))
                    .await
                {
                    self.analysis_failed(error.to_string());
                }
            }
            Err(error) => self.analysis_failed(error.to_string()),
        }
        if restart_review {
            self.continue_review().await;
        }
    }

    pub(super) fn coordinator(
        &self,
        engine: &EngineHandle,
        fingerprint: &str,
    ) -> Result<(AnalysisHandle, AnalysisSubscription), AppError> {
        let mut config = self.config.analysis.clone();
        config.engine_fingerprint = self.evaluation_fingerprint(fingerprint)?;
        config.live = self.live_settings();
        config.pondering = self.pondering();
        config.cache_enabled = self.settings.cache_enabled;
        config.restriction = self.engine_restriction();
        config.limits = self.settings.analysis_limits.into();
        Ok(analysis::spawn(engine.clone(), config)?)
    }
}
