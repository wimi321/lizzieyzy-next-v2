use super::Actor;
use crate::{
    AppError, GraphView, MistakeBarPoint, MoveLoss, MoveRankSettings, ProblemMoveSort,
    ProblemMoveView, ProblemMovesQuery, ProblemMovesView,
};
use document::{GamePosition, NodeId};
use go_core::Move;
use std::collections::HashMap;

impl Actor {
    pub(super) fn problem_moves(
        &self,
        query: ProblemMovesQuery,
    ) -> Result<ProblemMovesView, AppError> {
        let graph = GraphView::from_document(&self.document)?;
        let nodes: HashMap<_, _> = self.tree.nodes.iter().map(|node| (node.id, node)).collect();
        let mut error_bar = Vec::with_capacity(graph.points.len());
        let mut entries = Vec::new();
        for point in graph.points {
            let mv = nodes.get(&point.node_id).and_then(|node| node.mv);
            let evaluated = mv.and_then(|mv| self.move_loss(NodeId(point.node_id), mv.into()));
            let loss = evaluated.as_ref().map(|(_, loss)| loss.clone());
            if let Some((coordinate, loss)) = evaluated
                && let Some(mv) = mv
            {
                let color = Move::from(mv).color().into();
                if !loss.is_best
                    && loss.rank >= query.minimum_rank
                    && query.color.is_none_or(|filter| filter == color)
                {
                    entries.push(ProblemMoveView {
                        node_id: point.node_id,
                        move_number: point.move_number,
                        color,
                        coordinate,
                        loss,
                    });
                }
            }
            error_bar.push(MistakeBarPoint {
                node_id: point.node_id,
                move_number: point.move_number,
                mv,
                loss,
            });
        }
        entries.sort_by(|left, right| match query.sort {
            ProblemMoveSort::LossDesc => right
                .loss
                .winrate_loss
                .total_cmp(&left.loss.winrate_loss)
                .then_with(|| left.move_number.cmp(&right.move_number)),
            ProblemMoveSort::MoveAsc => left.move_number.cmp(&right.move_number),
            ProblemMoveSort::MoveDesc => right.move_number.cmp(&left.move_number),
        });
        Ok(ProblemMovesView {
            revision: self.revision,
            settings: self.settings.move_rank.clone(),
            query,
            entries,
            error_bar,
            current_index: graph.current_index,
        })
    }

    fn move_loss(&self, node: NodeId, mv: Move) -> Option<(String, MoveLoss)> {
        let parent = self.document.node(node).ok()?.parent()?;
        let before = self.review.results.get(&parent)?;
        let after = self.review.results.get(&node)?;
        let before_position = self.document.position_at(parent).ok()?;
        let after_position = self.document.position_at(node).ok()?;
        if before.key != before_position.analysis_key()
            || after.key != after_position.analysis_key()
            || !same_replay_context(&before_position, &after_position, mv)
        {
            return None;
        }
        let size = after_position.board.size();
        let loss = MoveLoss::between(
            mv,
            size,
            &before.response,
            &after.response,
            &self.settings.move_rank,
        )?;
        Some((mv.to_gtp(size).ok()?, loss))
    }

    pub(super) async fn set_move_rank_settings(
        &mut self,
        move_rank: MoveRankSettings,
    ) -> Result<MoveRankSettings, AppError> {
        move_rank.validate()?;
        if self.settings.move_rank != move_rank {
            let mut settings = self.settings.clone();
            settings.move_rank = move_rank.clone();
            settings.save(&self.directory).await?;
            self.settings = settings;
            // Existing subscribers re-read on document events, including ones
            // that only change derived data. No document edit or Focus command.
            self.publish_document()?;
        }
        Ok(move_rank)
    }
}

/// Do not blame a move for an accompanying SGF setup/turn reset. Its parent's
/// candidates describe another position. A later ordinary move is comparable.
fn same_replay_context(before: &GamePosition, after: &GamePosition, mv: Move) -> bool {
    before.board.to_play() == mv.color()
        && before.board.size() == after.board.size()
        && before.rules == after.rules
        && before.komi == after.komi
        && before.initial_stones == after.initial_stones
        && before.initial_player == after.initial_player
        && after.analysis_moves.last() == Some(&mv)
        && after.analysis_moves.len() == before.analysis_moves.len() + 1
        && after.analysis_moves[..before.analysis_moves.len()] == before.analysis_moves
}

#[cfg(test)]
mod tests;
