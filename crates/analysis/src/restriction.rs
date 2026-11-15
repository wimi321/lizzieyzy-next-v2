use crate::AnalysisError;
use document::GamePosition;
use engine_session::AnalysisRequest;
use go_core::{Color, Point};
use katago_protocol::{MoveRestriction, MoveRestrictions, Player};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestrictionKind {
    Allow,
    Avoid,
}

/// Runtime-only foreground filter, never part of a persisted analysis result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisRestriction {
    pub kind: RestrictionKind,
    pub points: Vec<Point>,
    pub depth: u32,
}

impl AnalysisRestriction {
    pub fn validate(&self) -> Result<(), AnalysisError> {
        if self.depth == 0 || self.points.is_empty() {
            return Err(AnalysisError::InvalidSettings(
                "restriction needs points and a positive depth",
            ));
        }
        Ok(())
    }

    pub(crate) fn apply(
        &self,
        request: &mut AnalysisRequest,
        position: &GamePosition,
    ) -> Result<(), AnalysisError> {
        self.validate()?;
        let entry = MoveRestriction {
            player: match position.board.to_play() {
                Color::Black => Player::Black,
                Color::White => Player::White,
            },
            moves: self
                .points
                .iter()
                .map(|point| point.to_gtp(position.board.size()))
                .collect::<Result<_, _>>()?,
            until_depth: self.depth,
        };
        let restrictions = match self.kind {
            RestrictionKind::Allow => MoveRestrictions {
                allow_moves: Some(vec![entry]),
                avoid_moves: None,
            },
            RestrictionKind::Avoid => MoveRestrictions {
                allow_moves: None,
                avoid_moves: Some(vec![entry]),
            },
        };
        crate::set_move_restrictions(request, &restrictions)
    }
}
