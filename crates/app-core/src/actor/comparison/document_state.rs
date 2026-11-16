//! Temporary boards keep document data without owning engine sessions or UI snapshots.

use super::*;
use crate::AppError;

pub(in crate::actor) struct DocumentState {
    results: HashMap<NodeId, StoredAnalysis>,
    fingerprint: Option<String>,
    replaced_raw: BTreeSet<NodeId>,
}

impl Actor {
    pub(in crate::actor) fn take_comparison_document(&mut self) -> DocumentState {
        DocumentState {
            results: std::mem::take(&mut self.comparison.results),
            fingerprint: self.comparison.fingerprint.clone(),
            replaced_raw: std::mem::take(&mut self.comparison.replaced_raw),
        }
    }

    pub(in crate::actor) fn restore_comparison_document(&mut self, mut saved: DocumentState) {
        saved.results.retain(|node, result| {
            self.document
                .position_at(*node)
                .is_ok_and(|position| position.analysis_key() == result.key)
                && if result.saved.is_some() {
                    self.settings.search_params.is_none()
                } else {
                    saved.fingerprint == self.comparison.fingerprint
                }
        });
        self.comparison.results = saved.results;
        // Changed engines invalidate evaluations, not the fact that a raw slot
        // was replaced. Keeping this marker prevents old file data resurfacing.
        self.comparison.replaced_raw = saved.replaced_raw;
    }

    pub(in crate::actor) async fn document_replaced_with_comparison(
        &mut self,
        saved: DocumentState,
    ) -> Result<(), AppError> {
        self.document_replaced_with_analysis(HashMap::new(), false, Some(saved))
            .await
    }
}
