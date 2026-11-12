use super::{Actor, Signal};
use crate::{AnalysisView, AppEvent};
use analysis::AnalysisStatus;
use document::NodeId;

impl Actor {
    pub(super) fn signal(&mut self, signal: Signal) {
        match signal {
            Signal::Snapshot(snapshot) => {
                if (self.pondering() || snapshot.source == analysis::SnapshotSource::Cache)
                    && let Some(focus) = &self.focus
                    && (focus.revision, focus.node_id, focus.key)
                        == (snapshot.revision, snapshot.node_id.0, snapshot.key)
                {
                    match AnalysisView::from_snapshot(*snapshot.clone(), focus.size, focus.to_play)
                    {
                        Ok(mut view) => {
                            self.retain_analysis(&snapshot);
                            // Graph history must not gate current candidates:
                            // a coordinator cache miss can publish a shallower
                            // search while the graph retains its best result.
                            let latest = view.summary;
                            view.summary = *self
                                .summaries
                                .entry(NodeId(view.node_id))
                                .and_modify(|best| {
                                    if latest.visits >= best.visits {
                                        *best = latest;
                                    }
                                })
                                .or_insert(latest);
                            if view.node_id == self.document.current().0 {
                                self.output.send(AppEvent::AnalysisUpdated(view));
                            } else if let Err(error) = self.publish_document() {
                                self.failed(error.to_string());
                            }
                        }
                        Err(error) => {
                            self.output.clear_analysis();
                            self.analysis_failed(format!("invalid analysis coordinate: {error}"));
                        }
                    }
                }
            }
            Signal::Engine(status) => {
                self.output
                    .send(AppEvent::EngineStatusChanged(status.into()));
            }
            Signal::Analysis(AnalysisStatus::Failed(error)) => {
                // Session failures carry richer diagnostics in EngineStatus.
                // Preserve those instead of replacing stderr with a query error.
                let session_failed = self.running.as_ref().is_some_and(|running| {
                    matches!(
                        *running.engine_status.borrow(),
                        engine_session::EngineStatus::Failed { .. }
                    )
                });
                if !session_failed {
                    self.failed(error.to_string());
                }
            }
            Signal::Analysis(_) => {}
        }
    }
}
