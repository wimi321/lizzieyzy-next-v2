//! Pure PV replay against an independent position; never edits the document.

use go_core::{Color, Move, Point};

use crate::{DocError, GameDocument, GamePosition, NodeId};

/// A preview stone still present in the final board, ready for a numbered label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewStone {
    pub point: Point,
    pub color: Color,
    /// One-based PV index. Passes consume a number but have no stone label.
    pub move_number: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariationPreview {
    /// Position after the legal prefix, including captures and updated history.
    pub position: GamePosition,
    /// Successfully applied PV moves, in order, including passes.
    pub applied_moves: Vec<Move>,
    /// Surviving preview stones in move order; captured stones have no labels.
    /// A point played again after capture receives its latest move number.
    pub stones: Vec<PreviewStone>,
    /// First invalid move's reason, attributed to the preview's base node. The
    /// invalid input is at applied_moves.len(); no moves after it are attempted.
    pub stop_reason: Option<DocError>,
}

impl GameDocument {
    /// Replay a PV from any live node without changing focus, revision, SGF,
    /// undo/redo history or analysis state. Errors in the base position fail the
    /// request; an invalid PV move returns the successfully applied prefix.
    /// Turns and full Go rules (including ko) are checked, unlike SGF replay.
    pub fn preview_variation(
        &self,
        node: NodeId,
        moves: &[Move],
    ) -> Result<VariationPreview, DocError> {
        let mut preview = VariationPreview {
            position: self.position_at(node)?,
            applied_moves: Vec::new(),
            stones: Vec::new(),
            stop_reason: None,
        };
        for &mv in moves {
            let position = &mut preview.position;
            let color = mv.color();
            if color != position.board.to_play() {
                preview.stop_reason = Some(DocError::WrongPlayer {
                    node,
                    expected: position.board.to_play(),
                    actual: color,
                });
                break;
            }
            let outcome = match position.board.play(mv) {
                Ok(outcome) => outcome,
                Err(reason) => {
                    preview.stop_reason = Some(DocError::IllegalMove { node, reason });
                    break;
                }
            };
            match color {
                Color::Black => {
                    position.captured_by.black += outcome.captured.len();
                    position.captured_by.white += outcome.self_captured.len();
                }
                Color::White => {
                    position.captured_by.white += outcome.captured.len();
                    position.captured_by.black += outcome.self_captured.len();
                }
            }
            preview.stones.retain(|stone| {
                !outcome.captured.contains(&stone.point)
                    && !outcome.self_captured.contains(&stone.point)
            });
            position.moves.push(mv);
            position.analysis_moves.push(mv);
            preview.applied_moves.push(mv);
            if let Move::Play(color, point) = mv
                && position.board.stone_at(point) == Some(color)
            {
                preview.stones.push(PreviewStone {
                    point,
                    color,
                    move_number: preview.applied_moves.len(),
                });
            }
        }
        Ok(preview)
    }
}

#[cfg(test)]
mod tests;
