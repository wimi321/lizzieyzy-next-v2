use super::{Actor, clear_analysis};
use crate::{AppError, AppEvent, DocumentView};
use document::NodeId;

impl Actor {
    pub(super) fn publish_document(&mut self) -> Result<(), AppError> {
        self.tree.refresh(&self.document, self.revision)?;
        self.refresh_graph_analysis();
        let mut view = DocumentView::from_document(&self.document, self.tree.revision)?;
        view.file_name = self.file_name.clone();
        view.file_path = self
            .file_state
            .path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        view.modified = self.file_state.modified(&self.document);
        view.can_undo = self.file_state.analysis_edits.can_undo(&self.document);
        view.can_redo = self.file_state.analysis_edits.can_redo(&self.document);
        view.warnings.retain(|warning| {
            !self.is_analysis_cleared(NodeId(warning.node))
                || !clear_analysis::is_analysis(&warning.property)
        });
        view.warnings.extend(
            self.file_state
                .save_warnings
                .iter()
                .cloned()
                .map(Into::into),
        );
        view.trial = self.move_trial.is_some();
        view.analysis_restriction = self.restriction.clone();
        view.restricted_analysis = self
            .restricted_analysis
            .clone()
            .filter(|v| v.revision == self.revision && v.node_id == self.document.current().0)
            .map(Box::new);
        view.scoring = self.scoring_view()?.map(Box::new);
        view.tree.comment_revision = self.tree.comment_revision;
        for point in &mut view.graph.points {
            let node = NodeId(point.node_id);
            point.summary = self
                .graph_analysis
                .valid_nodes
                .contains(&node)
                .then(|| self.summaries.get(&node).copied())
                .flatten();
        }
        self.output.send(AppEvent::DocumentChanged {
            revision: self.revision,
            view,
        });
        // Document events coalesce in the mailbox. Keep the matching result
        // after the document so a slow consumer can establish its revision.
        self.output
            .republish_analysis(self.revision, self.document.current().0);
        self.publish_saved_focus();
        Ok(())
    }
}
