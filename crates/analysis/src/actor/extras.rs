use super::{Active, Actor, Identity, QueryKind};
use crate::{
    AnalysisError, AnalysisExtras, AnalysisSnapshot, AnalysisStatus, SnapshotSource, request,
    speed::SearchSpeed,
};
use katago_protocol::AnalysisResponse;

impl Actor {
    pub(super) async fn start(&mut self) -> Result<(), AnalysisError> {
        if self.active.is_some() || self.focus.is_none() {
            return Ok(());
        }
        let kind = if self.config.pure_network {
            if self.extras_attempted {
                return Ok(());
            }
            QueryKind::PureNetwork
        } else if self.needs_extras() {
            QueryKind::Extras
        } else if self.config.pondering && self.limited.is_none() && !self.live_finished {
            QueryKind::Live
        } else {
            return Ok(());
        };
        let Some(focus) = &self.focus else {
            return Ok(());
        };
        let mut query = request::build(&focus.position, &self.config.live)?;
        query.include_policy = (self.config.live.include_policy
            || self.config.extras.policy
            || kind == QueryKind::PureNetwork)
            .then_some(true);
        query.include_moves_ownership = self.config.extras.moves_ownership.then_some(true);
        if kind != QueryKind::Live {
            query.max_visits = Some(1);
            query.report_during_search_every = None;
            query.first_report_during_search_after = None;
            if kind == QueryKind::PureNetwork {
                query.include_ownership = Some(true);
            }
        }
        let subscription = self.engine.submit(query).await?;
        self.extras_attempted |= kind != QueryKind::Live;
        self.active = Some(Active {
            kind,
            identity: Identity::from(focus),
            subscription,
            speed: SearchSpeed::default(),
            started: None,
            visits: None,
        });
        // One-shot work must not erase the user's limit-pause reason.
        self.status.send_replace(
            self.limited
                .map_or(AnalysisStatus::Analyzing, AnalysisStatus::Limited),
        );
        Ok(())
    }

    fn needs_extras(&self) -> bool {
        if self.extras_attempted
            || (!self.config.extras.policy && !self.config.extras.moves_ownership)
        {
            return false;
        }
        self.throttle
            .pending()
            .or(self.last_published.as_ref())
            .is_none_or(|snapshot| {
                (self.config.extras.policy && snapshot.response.policy.is_none())
                    || (self.config.extras.moves_ownership
                        && snapshot
                            .response
                            .move_infos
                            .iter()
                            .any(|candidate| candidate.ownership.is_none()))
            })
    }

    /// Same-focus intent changes must not consume the only copy of a completed
    /// supplement/raw result before AppCore has received it. Ordinary live
    /// frames retain the established cancellation/clear behavior.
    pub(super) async fn cancel_preserving_transient(&mut self) -> Result<(), AnalysisError> {
        let transient = |snapshot: &&AnalysisSnapshot| {
            matches!(
                snapshot.source,
                SnapshotSource::Extras | SnapshotSource::PureNetwork
            )
        };
        let pending = self.throttle.pending().filter(transient);
        let completed_pending = pending.is_some();
        let snapshot = pending
            .or(self.last_published.as_ref().filter(transient))
            .cloned();
        let attempted = self.extras_attempted;
        self.cancel().await?;
        if completed_pending {
            // The pending payload is being retained, not discarded. Remember
            // completion even if this engine omitted an optional output field.
            self.extras_attempted = attempted;
        }
        if let Some(snapshot) = snapshot {
            self.offer(snapshot);
        }
        Ok(())
    }

    pub(super) async fn set_extras(&mut self, extras: AnalysisExtras) -> Result<(), AnalysisError> {
        if self.config.extras == extras {
            return Ok(());
        }
        self.config.extras = extras;
        self.extras_attempted = false;
        self.output.clear();
        self.cancel_preserving_transient().await?;
        self.start().await
    }

    pub(super) async fn set_pure_network(&mut self, enabled: bool) -> Result<(), AnalysisError> {
        if self.config.pure_network == enabled {
            return Ok(());
        }
        self.cancel().await?;
        self.output.clear();
        self.throttle.reset();
        self.extras_attempted = false;
        self.display_floor = None;
        self.config.pure_network = enabled;
        if enabled {
            self.ordinary_display = self.last_published.take();
        } else {
            self.last_published = None;
            if let Some(mut snapshot) = self.ordinary_display.take()
                && let Some(focus) = &self.focus
                && snapshot.key == focus.key
                && snapshot.node_id == focus.node_id
            {
                snapshot.revision = focus.revision;
                // Restoring a paused ordinary frame is not a new live result.
                snapshot.source = SnapshotSource::Cache;
                snapshot.visits_per_second = None;
                self.offer(snapshot);
            } else {
                self.offer_cached();
            }
        }
        self.start().await
    }

    pub(super) fn supplemental_result(&mut self, kind: QueryKind, mut response: AnalysisResponse) {
        if response.is_during_search {
            return;
        }
        if kind == QueryKind::PureNetwork {
            if let Some(winrate) = response.root_info.raw_winrate {
                response.root_info.winrate = winrate;
            }
            if let Some(lead) = response.root_info.raw_lead {
                response.root_info.score_lead = lead;
            }
        }
        let Some(focus) = &self.focus else { return };
        let source = if kind == QueryKind::PureNetwork {
            SnapshotSource::PureNetwork
        } else {
            SnapshotSource::Extras
        };
        let response = if kind == QueryKind::Extras {
            self.last_published.as_ref().map_or_else(
                || response.clone(),
                |previous| merge_extras(&previous.response, &response, self.config.extras),
            )
        } else {
            response
        };
        let snapshot = AnalysisSnapshot {
            revision: focus.revision,
            node_id: focus.node_id,
            key: focus.key,
            response,
            source,
            visits_per_second: None,
        };
        self.active = None;
        self.status.send_replace(
            self.limited
                .map_or(AnalysisStatus::Idle, AnalysisStatus::Limited),
        );
        self.offer(snapshot);
        self.resume_after_extras = kind == QueryKind::Extras;
    }
}

fn merge_extras(
    previous: &AnalysisResponse,
    supplement: &AnalysisResponse,
    extras: AnalysisExtras,
) -> AnalysisResponse {
    let mut response = previous.clone();
    if extras.policy {
        response.policy.clone_from(&supplement.policy);
    }
    if extras.moves_ownership {
        for candidate in &mut response.move_infos {
            candidate.ownership = supplement
                .move_infos
                .iter()
                .find(|other| other.coordinate == candidate.coordinate)
                .and_then(|other| other.ownership.clone());
        }
    }
    response
}
