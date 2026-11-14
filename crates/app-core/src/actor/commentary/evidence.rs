use super::*;
use ::commentary::{Candidate, PositionInput};
use std::collections::HashMap;

impl Actor {
    pub(super) fn commentary_evidence(
        &self,
        mode: &CommentaryMode,
        language: ::commentary::Language,
    ) -> Result<(::commentary::Evidence, NodeId), AppError> {
        let nodes: HashMap<_, _> = self.tree.nodes.iter().map(|n| (n.id, n)).collect();
        let mut main = Vec::new();
        let mut id = self.document.root().0;
        while let Some(node) = nodes.get(&id) {
            main.push(*node);
            let Some(next) = node.children.first() else {
                break;
            };
            id = *next;
        }
        let (positions, target) = match mode {
            CommentaryMode::NextMove => (vec![self.document.current()], self.document.current()),
            CommentaryMode::Range { from, to } => {
                if *from == 0
                    || from > to
                    || main.last().is_none_or(|n| n.move_number < *to as usize)
                {
                    return Err(CommentaryError::InvalidRange.into());
                }
                let chosen: Vec<_> = main
                    .iter()
                    .filter(|n| {
                        n.mv.is_some()
                            && n.move_number >= *from as usize
                            && n.move_number <= *to as usize
                    })
                    .collect();
                let target = chosen.first().ok_or(CommentaryError::InvalidRange)?.id;
                (
                    chosen.iter().filter_map(|n| n.parent.map(NodeId)).collect(),
                    NodeId(target),
                )
            }
            CommentaryMode::WholeGame => (
                main.iter()
                    .filter(|n| n.mv.is_some())
                    .filter_map(|n| n.parent.map(NodeId))
                    .collect(),
                self.document.root(),
            ),
        };
        let inputs: Vec<_> = positions
            .into_iter()
            .filter_map(|id| self.commentary_position(id).transpose())
            .collect::<Result<_, _>>()?;
        let evidence = ::commentary::build_evidence(
            &inputs,
            !matches!(mode, CommentaryMode::NextMove),
            language,
        )
        .map_err(crate::commentary::domain_error)?;
        Ok((evidence, target))
    }

    fn commentary_position(&self, node: NodeId) -> Result<Option<PositionInput>, AppError> {
        let Some(stored) = self.stored_analysis(node.0) else {
            return Ok(None);
        };
        if stored.response.move_infos.is_empty() {
            return Ok(None);
        }
        let position = self.document.position_at(node)?;
        let size = position.board.size();
        let mut candidates = Vec::new();
        for candidate in &stored.response.move_infos {
            let mv = go_core::Move::from_gtp(position.board.to_play(), &candidate.coordinate, size)
                .map_err(|_| CommentaryError::InvalidEvidence)?;
            let mut color = position.board.to_play();
            let mut pv = Vec::new();
            for coordinate in candidate.pv.iter().take(12) {
                pv.push(
                    go_core::Move::from_gtp(color, coordinate, size)
                        .map_err(|_| CommentaryError::InvalidEvidence)?,
                );
                color = color.opponent();
            }
            candidates.push(Candidate {
                mv,
                order: candidate.order,
                visits: candidate.visits,
                winrate: candidate.winrate,
                score_lead: stored
                    .candidate_has_score(&candidate.coordinate)
                    .then_some(candidate.score_lead),
                pv,
            });
        }
        let mut continuation = Vec::new();
        let mut current = node;
        while continuation.len() < 5 {
            let Some(child) = self.document.node(current)?.children().first().copied() else {
                break;
            };
            let record = self.document.node(child)?;
            if record
                .properties()
                .iter()
                .any(|property| matches!(property.identifier.as_str(), "AB" | "AW" | "AE" | "PL"))
            {
                break;
            }
            let Some(view) = self.tree.nodes.iter().find(|n| n.id == child.0) else {
                break;
            };
            if view.move_error.is_some() {
                break;
            }
            if let Some(mv) = view.mv {
                continuation.push(mv.into());
            }
            current = child;
        }
        Ok(Some(PositionInput {
            node_id: node.0,
            move_number: u32::try_from(position.moves.len())
                .map_err(|_| CommentaryError::InvalidEvidence)?,
            board: position.board,
            root_visits: stored.response.root_info.visits,
            actual_move: continuation.first().copied(),
            continuation,
            candidates,
        }))
    }
}
