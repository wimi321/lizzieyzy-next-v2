//! Pure construction of Java-compatible local life-and-death positions.
//!
//! This deliberately preserves the Java attack-side count fallback and its
//! asymmetric area arithmetic. It does not play moves or apply Go captures.
use crate::{BoardSize, BoardSizeError, Color, Point};
use thiserror::Error;

mod wall;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TsumegoToPlay {
    #[default]
    Inherit,
    Black,
    White,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TsumegoAttacker {
    #[default]
    Auto,
    Black,
    White,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TsumegoKoThreat {
    #[default]
    Both,
    Attacker,
    Defender,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsumegoOptions {
    pub to_play: TsumegoToPlay,
    pub attacker: TsumegoAttacker,
    pub ko_threat: TsumegoKoThreat,
    pub wall_distance: u32,
}

impl Default for TsumegoOptions {
    fn default() -> Self {
        Self {
            to_play: TsumegoToPlay::Inherit,
            attacker: TsumegoAttacker::Auto,
            ko_threat: TsumegoKoThreat::Both,
            wall_distance: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TsumegoOutput {
    /// Complete board, in row-major order. Includes unchanged problem stones.
    pub stones: Vec<Option<Color>>,
    /// Java start-stone list, in construction order, after ko replacements.
    pub added_stones: Vec<(Color, Point)>,
    pub attacker: Color,
    pub to_play: Color,
    /// None means all four walls are beyond the board: Java makes no change.
    pub komi: Option<f64>,
    /// Colors whose requested ko threats could not fit; Java warning order.
    pub ko_warnings: Vec<Color>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TsumegoError {
    #[error(transparent)]
    BoardSize(#[from] BoardSizeError),
    #[error("board has {actual} points, expected {expected}")]
    BoardLength { actual: usize, expected: usize },
    #[error("tsumego wall distance must be positive")]
    WallDistance,
}

/// Construct walls, area-balancing stones and optional ko threats.
///
/// Raw stones permit the same editing positions as Java, including groups with
/// no liberties. The document layer decides whether a result is analyzable.
/// Inputs are never mutated, including on errors or Java's no-op path.
pub fn generate(
    size: BoardSize,
    stones: &[Option<Color>],
    to_play: Color,
    options: TsumegoOptions,
) -> Result<TsumegoOutput, TsumegoError> {
    size.validate()?;
    if stones.len() != size.area() {
        return Err(TsumegoError::BoardLength {
            actual: stones.len(),
            expected: size.area(),
        });
    }
    if options.wall_distance == 0 {
        return Err(TsumegoError::WallDistance);
    }
    let width = i64::from(size.width);
    let height = i64::from(size.height);
    let (mut left, mut right, mut top, mut bottom) = (width - 1, 0, height - 1, 0);
    let (mut black, mut white) = (0, 0);
    for x in 0..size.width {
        for y in 0..size.height {
            if let Some(color) = stones[usize::from(y) * usize::from(size.width) + usize::from(x)] {
                match color {
                    Color::Black => black += 1,
                    Color::White => white += 1,
                }
                left = left.min(i64::from(x));
                right = right.max(i64::from(x));
                top = top.min(i64::from(y));
                bottom = bottom.max(i64::from(y));
            }
        }
    }
    // Java initializes each color's bounds to the whole board, then only
    // expands them. Neither can expand, so all cover-count comparisons tie.
    let attacker = match options.attacker {
        TsumegoAttacker::Black => Color::Black,
        TsumegoAttacker::White => Color::White,
        TsumegoAttacker::Auto if black >= white => Color::Black,
        TsumegoAttacker::Auto => Color::White,
    };
    let gap = i64::from(options.wall_distance) + 1;
    left = if left - gap > 0 { left - gap } else { -1 };
    right = if right + gap < width - 1 {
        right + gap
    } else {
        width
    };
    top = if top - gap > 0 { top - gap } else { -1 };
    bottom = if bottom + gap < height - 1 {
        bottom + gap
    } else {
        height
    };
    let mut result = TsumegoOutput {
        stones: stones.to_vec(),
        added_stones: Vec::new(),
        attacker,
        to_play,
        komi: None,
        ko_warnings: Vec::new(),
    };
    if left == -1 && right == width && top == -1 && bottom == height {
        return Ok(result);
    }
    let mut board = wall::WallBoard::new(size, &mut result);
    board.add_wall(left, right, top, bottom);
    board.fill(left, right, top, bottom, options.ko_threat);
    result.to_play = match options.to_play {
        TsumegoToPlay::Inherit => to_play,
        TsumegoToPlay::Black => Color::Black,
        TsumegoToPlay::White => Color::White,
    };
    Ok(result)
}

#[cfg(test)]
mod tests;
