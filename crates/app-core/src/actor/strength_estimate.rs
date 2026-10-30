use super::Actor;
use crate::{AppError, ColorView, ProblemMovesQuery, StrengthEstimateView};
use document::NodeId;
use go_core::Move;
use strength::{CandidateInput, Estimator, MoveInput};

impl Actor {
    pub(super) fn strength_estimate(&mut self) -> Result<StrengthEstimateView, AppError> {
        // P1-24 validates both retained keys, replay context and finite losses.
        // Its unfiltered bar follows the same displayed line as the graph.
        let losses = self.problem_moves(ProblemMovesQuery::default())?;
        let total_moves = losses
            .error_bar
            .iter()
            .filter(|point| point.mv.is_some())
            .count();
        let mut inputs = Vec::new();
        for point in losses.error_bar {
            let (Some(mv), Some(loss)) = (point.mv, point.loss) else {
                continue;
            };
            let node = NodeId(point.node_id);
            let Some(parent) = self.document.node(node)?.parent() else {
                continue;
            };
            let Some(before) = self.review.results.get(&parent) else {
                continue;
            };
            let Some(after) = self.review.results.get(&node) else {
                continue;
            };
            let position = self.document.position_at(node)?;
            let mv = Move::from(mv);
            let is_black = ColorView::from(mv.color()) == ColorView::Black;
            // Java strength uses candidate-to-candidate losses when available,
            // otherwise root-to-root. P1-24 supplies comparability and rank
            // settings, but its best-candidate-to-root fallback is different.
            let before_root = &before.response.root_info;
            let after_root = &after.response.root_info;
            if !before_root.winrate.is_finite()
                || !(0.0..=1.0).contains(&before_root.winrate)
                || !before_root.score_lead.is_finite()
            {
                continue;
            }
            let sign = if is_black { 1.0 } else { -1.0 };
            let mut candidates: Vec<_> = before.response.move_infos.iter().collect();
            candidates.sort_by_key(|candidate| candidate.order);
            let Ok(move_number) = u32::try_from(point.move_number) else {
                continue;
            };
            let mut input = MoveInput {
                move_number,
                is_black,
                coordinate: mv
                    .to_gtp(position.board.size())
                    .map_err(analysis::AnalysisError::from)?,
                winrate_loss: sign * (before_root.winrate - after_root.winrate) * 100.0,
                score_loss: Some(sign * (before_root.score_lead - after_root.score_lead)),
                move_rank: loss.rank.severity_level(),
                candidates: candidates
                    .into_iter()
                    .map(|candidate| CandidateInput {
                        coordinate: candidate.coordinate.clone(),
                        winrate: if is_black {
                            candidate.winrate
                        } else {
                            1.0 - candidate.winrate
                        } * 100.0,
                        score_lead: Some(if is_black {
                            candidate.score_lead
                        } else {
                            -candidate.score_lead
                        }),
                        prior: candidate.prior,
                        order: candidate.order,
                    })
                    .collect(),
            };
            if let Some(sample) = strength::Sample::from_input(&input) {
                // Reuse P1-24's configurable classifier with the Java estimator's
                // candidate-based losses, keeping display tiers consistent.
                input.move_rank = self
                    .settings
                    .move_rank
                    .classify_losses(sample.winrate_loss, sample.score_loss)
                    .severity_level();
                inputs.push(input);
            }
        }
        // The model is immutable and cached by this actor. Reports are rebuilt
        // from current retained results, so no second invalidation cache exists.
        let estimator = match &mut self.strength {
            Some(estimator) => estimator,
            None => self.strength.insert(Estimator::bundled()?),
        };
        let report = estimator.estimate(&inputs);
        Ok(StrengthEstimateView {
            revision: self.revision,
            total_moves,
            analyzed_moves: report.overall.sample_count,
            report,
        })
    }
}

#[cfg(test)]
mod tests;
