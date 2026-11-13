//! Application integration for the optional, failure-isolated SQLite actor.

use super::{Actor, Signal};
use crate::{AnalysisView, AppError};
use analysis::{AnalysisSnapshot, SnapshotSource};
use document::{GamePosition, NodeId};
use katago_protocol::AnalysisResponse;

impl Actor {
    /// Version the persisted namespace independently of the in-memory profile
    /// identity. Include the runtime recommendations and requested payload shape.
    fn cache_fingerprint(&self) -> Option<String> {
        let fingerprint = &self.running.as_ref()?.fingerprint;
        let mut hash = blake3::Hasher::new();
        hash.update(b"lizzieyzy-next2:persistent-analysis:v1\0");
        hash.update(fingerprint.as_bytes());
        let benchmark = self.settings.benchmarks.get(fingerprint);
        let encoded = serde_json::to_vec(&benchmark).ok()?;
        hash.update(&encoded);
        hash.update(&[
            u8::from(self.config.analysis.live.include_ownership),
            u8::from(self.config.analysis.live.include_policy),
        ]);
        Some(hash.finalize().to_hex().to_string())
    }

    pub(super) async fn set_cache_enabled(&mut self, enabled: bool) -> Result<(), AppError> {
        if self.settings.cache_enabled == enabled {
            return Ok(());
        }
        let mut settings = self.settings.clone();
        settings.cache_enabled = enabled;
        settings.save(&self.directory).await?;
        self.settings = settings;
        if let Some(cache) = &self.cache {
            cache.set_enabled(enabled).await;
        }
        if let Some(running) = &self.running {
            running.coordinator.set_cache_enabled(enabled).await?;
        }
        Ok(())
    }

    pub(super) async fn clear_cache(&mut self, all: bool) -> Result<(), AppError> {
        if let Some(cache) = &self.cache {
            if all {
                cache.clear_all().await;
            } else {
                let position = self.document.position_at(self.document.current())?;
                cache.clear_position(position.analysis_key()).await;
            }
        }
        Ok(())
    }

    pub(super) async fn cached_focus(
        &mut self,
        node: NodeId,
        position: &GamePosition,
    ) -> Option<AnalysisResponse> {
        if !self.settings.cache_enabled {
            return None;
        }
        let key = position.analysis_key();
        let retained = self
            .review
            .results
            .get(&node)
            .filter(|stored| stored.key == key && stored.saved.is_none())
            .map(|stored| stored.response.clone());
        if self.is_analysis_cleared(node) {
            return retained;
        }
        let Some(fingerprint) = self.cache_fingerprint() else {
            return retained;
        };
        let persisted = match &self.cache {
            Some(cache) => cache.get(key, &fingerprint).await,
            None => None,
        };
        // A damaged payload must not become an engine failure just because its
        // SQLite pages and JSON happened to remain readable.
        let mut persisted = persisted;
        if let Some(response) = &persisted {
            let checked = AnalysisView::from_snapshot(
                AnalysisSnapshot {
                    revision: self.revision,
                    node_id: node,
                    key,
                    response: response.clone(),
                    source: SnapshotSource::Cache,
                    visits_per_second: None,
                },
                position.board.size(),
                position.board.to_play(),
            );
            if let Err(error) = checked {
                if let Some(cache) = &self.cache {
                    cache
                        .invalidate(&format!("invalid cached coordinates: {error}"))
                        .await;
                }
                persisted = None;
            }
        }
        match (retained, persisted) {
            (Some(retained), Some(persisted))
                if retained.root_info.visits >= persisted.root_info.visits =>
            {
                Some(retained)
            }
            (_, Some(persisted)) => Some(persisted),
            (retained, None) => retained,
        }
    }

    // These barriers run only at lifecycle boundaries, never per live snapshot.
    pub(super) async fn flush_cached_focus(&self) {
        if let Some(cache) = &self.cache
            && let Some(focus) = &self.focus
        {
            cache.flush_position(focus.key).await;
        }
    }

    pub(super) async fn flush_cached_analysis(&self) {
        if let Some(cache) = &self.cache {
            cache.flush().await;
        }
    }

    pub(super) fn persist_analysis(&self, snapshot: &AnalysisSnapshot) {
        if self.settings.cache_enabled
            && let Some(cache) = &self.cache
            && let Some(fingerprint) = self.cache_fingerprint()
        {
            cache.put(snapshot.key, &fingerprint, snapshot.response.clone());
        }
    }

    pub(super) fn cache_signal(&mut self, signal: Signal) {
        let snapshot = match &signal {
            Signal::Snapshot(snapshot) if snapshot.source == SnapshotSource::Live => {
                Some(snapshot.clone())
            }
            _ => None,
        };
        self.signal(signal);
        // Only persist a validated result actually accepted by the existing
        // application pipeline. Rejected revisions/coordinates never enter SQLite.
        if let Some(snapshot) = snapshot
            && self
                .review
                .results
                .get(&snapshot.node_id)
                .is_some_and(|stored| {
                    stored.saved.is_none()
                        && stored.key == snapshot.key
                        && stored.response == snapshot.response
                })
        {
            self.persist_analysis(&snapshot);
        }
    }

    pub(super) async fn restore_cached_review(&mut self) -> Result<(), AppError> {
        // The line already belongs to a validated review plan. Seed all nodes so
        // reopening a game can skip completed work, including background results.
        for (node, position) in self.review.line.clone() {
            if let Some(response) = self.cached_focus(node, &position).await {
                let snapshot = AnalysisSnapshot {
                    revision: self.revision,
                    node_id: node,
                    key: position.analysis_key(),
                    response,
                    source: SnapshotSource::Cache,
                    visits_per_second: None,
                };
                if self.preserves_saved_analysis(&snapshot) {
                    continue;
                }
                let view = AnalysisView::from_snapshot(
                    snapshot.clone(),
                    position.board.size(),
                    position.board.to_play(),
                )
                .map_err(analysis::AnalysisError::from)?;
                self.retain_analysis(&snapshot);
                self.summaries.insert(node, view.summary);
            }
        }
        self.publish_document()
    }
}

#[cfg(test)]
mod tests;
