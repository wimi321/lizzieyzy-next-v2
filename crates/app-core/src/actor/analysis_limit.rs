use super::Actor;
use crate::{AnalysisLimitSettings, AnalysisPauseReason, AnalysisState, AppError, AppEvent};
use analysis::{AnalysisStatus, LimitReason};
use document::NodeId;
use std::collections::HashSet;

impl Actor {
    pub(super) fn analysis_state(&self) -> AnalysisState {
        let pause_reason = if !self.pondering() {
            // Benchmark is a separate temporary operation, not a user pause.
            (!self.benchmark_active()).then_some(AnalysisPauseReason::User)
        } else {
            self.limit_reason().map(|reason| match reason {
                LimitReason::Time => AnalysisPauseReason::TimeLimit,
                LimitReason::Playouts => AnalysisPauseReason::PlayoutLimit,
            })
        };
        AnalysisState {
            pondering: self.pondering() && pause_reason.is_none(),
            pause_reason,
        }
    }

    pub(super) fn limit_reason(&self) -> Option<LimitReason> {
        self.running
            .as_ref()
            .and_then(|running| match *running.analysis_status.borrow() {
                AnalysisStatus::Limited(reason) => Some(reason),
                _ => None,
            })
    }

    pub(super) fn publish_analysis_state(&self) {
        self.output
            .send(AppEvent::AnalysisStateChanged(self.analysis_state()));
    }

    pub(super) async fn set_analysis_limits(
        &mut self,
        limits: AnalysisLimitSettings,
    ) -> Result<AnalysisLimitSettings, AppError> {
        limits.validate()?;
        if self.settings.analysis_limits != limits {
            let mut settings = self.settings.clone();
            settings.analysis_limits = limits;
            settings.save(&self.directory).await?;
            self.settings = settings;
            if let Some(running) = &self.running {
                running.coordinator.set_limits(limits.into()).await?;
            }
        }
        Ok(self.settings.analysis_limits)
    }

    pub(super) async fn set_pondering(&mut self, enabled: bool) -> Result<(), AppError> {
        self.ensure_no_benchmark()?;
        let changed = enabled != self.settings.pondering;
        if changed {
            let mut settings = self.settings.clone();
            settings.pondering = enabled;
            settings.save(&self.directory).await?;
            self.settings = settings;
        }
        let sync_changed = self.sync_active() && self.board_sync.analysis_paused == enabled;
        let limited = self.limit_reason().is_some();
        if changed || sync_changed || limited {
            if self.sync_active() {
                self.board_sync.analysis_paused = !enabled;
            }
            // Automatic stopping itself never clears the last analysis event.
            self.output.clear_analysis();
            if enabled && self.review_running() {
                self.stop_review_query().await?;
            }
            if let Some(running) = &self.running {
                running
                    .coordinator
                    .set_pondering(enabled && self.focus.is_some())
                    .await?;
                if enabled && limited && self.focus.is_some() {
                    running.coordinator.resume().await?;
                }
            }
            if enabled {
                self.continue_review().await;
            }
            self.publish_saved_focus();
        }
        Ok(())
    }

    /// Java's forward navigation replays playMove; backward navigation only
    /// undoes. A branch jump resumes if its downward path plays a real move.
    pub(super) fn navigation_plays_move(&self, before: NodeId) -> bool {
        self.navigation_between_plays_move(before, self.document.current())
    }

    pub(super) fn navigation_between_plays_move(&self, before: NodeId, target: NodeId) -> bool {
        let mut ancestors = HashSet::new();
        let mut cursor = Some(before);
        while let Some(id) = cursor {
            ancestors.insert(id);
            cursor = self.document.node(id).ok().and_then(|node| node.parent());
        }
        let mut cursor = Some(target);
        let mut played = false;
        while let Some(id) = cursor {
            if ancestors.contains(&id) {
                break;
            }
            let Ok(node) = self.document.node(id) else {
                return false;
            };
            let properties = node.properties();
            played |= properties
                .iter()
                .any(|p| matches!(p.identifier.as_str(), "B" | "W"))
                && !properties
                    .iter()
                    .any(|p| matches!(p.identifier.as_str(), "AB" | "AW" | "AE" | "PL"));
            cursor = node.parent();
        }
        played && self.document.position_at(target).is_ok()
    }
}
