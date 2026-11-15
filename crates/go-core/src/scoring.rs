//! Synchronous, explicit final-position scoring. Life/death is supplied by the
//! caller; empty regions touching both colors (or neither) are neutral.

use crate::{BoardSize, Color, Point, Position, RulePreset, ScoringRule};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ScoringError {
    #[error("scoring point {0:?} is outside the board")]
    OutOfBounds(Point),
    #[error("dead-stone point {0:?} has no stone")]
    EmptyDeadPoint(Point),
    #[error("neutral point {0:?} is occupied")]
    OccupiedNeutralPoint(Point),
    #[error("komi must be finite")]
    InvalidKomi,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScoreBreakdown {
    /// Surviving stones of this color.
    pub living_stones: usize,
    pub territory: usize,
    /// Dead stones of this color, awarded as prisoners to the opponent.
    pub dead_stones: usize,
    /// Opposing stones captured before scoring started, excluding marked dead.
    pub captures: usize,
    pub komi: f64,
    pub handicap_compensation: u32,
    pub total: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Score {
    /// Row-major, top-left first. Living stones retain their own color;
    /// removed dead stones are scored as empty intersections.
    pub ownership: Vec<Option<Color>>,
    pub black: ScoreBreakdown,
    pub white: ScoreBreakdown,
    /// Black minus White, including komi and handicap compensation.
    pub score_lead: f64,
    /// Standard SGF RE notation, including `0` for a draw.
    pub result: String,
}

#[derive(Clone, Copy, Debug)]
pub struct ScoreOptions {
    pub rule: ScoringRule,
    /// Original SGF preset determines handicap compensation, even when the
    /// user temporarily changes the counting rule.
    pub preset: RulePreset,
    pub komi: f64,
    pub handicap: u32,
    /// Stones already captured by [Black, White].
    pub captured_by: [usize; 2],
}

/// Score with all declared dead stones removed. A marked neutral point removes
/// only its own point from the score; it does not split an empty region or act
/// as an artificial boundary. Inputs are never mutated.
pub fn score(
    position: &Position,
    dead_stones: &HashSet<Point>,
    neutral_points: &HashSet<Point>,
    options: ScoreOptions,
) -> Result<Score, ScoringError> {
    if !options.komi.is_finite() {
        return Err(ScoringError::InvalidKomi);
    }
    let size = position.size();
    for &point in dead_stones.iter().chain(neutral_points.iter()) {
        if !size.contains(point) {
            return Err(ScoringError::OutOfBounds(point));
        }
    }
    let mut stones = position.stones().to_vec();
    let mut sides = [ScoreBreakdown::default(); 2];
    for &point in dead_stones {
        let color = position
            .stone_at(point)
            .ok_or(ScoringError::EmptyDeadPoint(point))?;
        sides[color_index(color)].dead_stones += 1;
        stones[index(size, point)] = None;
    }
    for &point in neutral_points {
        if position.stone_at(point).is_some() {
            return Err(ScoringError::OccupiedNeutralPoint(point));
        }
    }
    let mut ownership = stones.clone();
    let mut visited = vec![false; size.area()];
    for start in 0..size.area() {
        if let Some(color) = stones[start] {
            sides[color_index(color)].living_stones += 1;
            continue;
        }
        if visited[start] {
            continue;
        }
        let mut region = vec![start];
        let mut boundary = [false; 2];
        visited[start] = true;
        let mut cursor = 0;
        while cursor < region.len() {
            for adjacent in neighbors(size, region[cursor]) {
                if let Some(color) = stones[adjacent] {
                    boundary[color_index(color)] = true;
                } else if !visited[adjacent] {
                    visited[adjacent] = true;
                    region.push(adjacent);
                }
            }
            cursor += 1;
        }
        let owner = match boundary {
            [true, false] => Some(Color::Black),
            [false, true] => Some(Color::White),
            _ => None,
        };
        for member in region {
            if !neutral_points.contains(&point(size, member)) {
                ownership[member] = owner;
                if let Some(color) = owner {
                    sides[color_index(color)].territory += 1;
                }
            }
        }
    }
    for (side, captures) in sides.iter_mut().zip(options.captured_by) {
        side.captures = captures;
    }
    sides[1].komi = options.komi;
    if options.rule == ScoringRule::Area {
        sides[1].handicap_compensation = match options.preset {
            RulePreset::Chinese => options.handicap,
            RulePreset::Aga => options.handicap.saturating_sub(1),
            _ => 0,
        };
    }
    for i in 0..2 {
        let points = match options.rule {
            ScoringRule::Area => sides[i].living_stones as f64 + sides[i].territory as f64,
            ScoringRule::Territory => {
                sides[i].territory as f64
                    + sides[i].captures as f64
                    + sides[1 - i].dead_stones as f64
            }
        };
        sides[i].total = points + sides[i].komi + f64::from(sides[i].handicap_compensation);
    }
    let score_lead = sides[0].total - sides[1].total;
    Ok(Score {
        ownership,
        black: sides[0],
        white: sides[1],
        score_lead,
        result: result_text(score_lead, options.komi),
    })
}

/// All orthogonally connected stones of the point's color, in board order.
pub fn stone_group(position: &Position, start: Point) -> Vec<Point> {
    let Some(color) = position.stone_at(start) else {
        return Vec::new();
    };
    let size = position.size();
    let mut visited = vec![false; size.area()];
    let mut group = vec![index(size, start)];
    visited[group[0]] = true;
    let mut cursor = 0;
    while cursor < group.len() {
        for adjacent in neighbors(size, group[cursor]) {
            if !visited[adjacent] && position.stones()[adjacent] == Some(color) {
                visited[adjacent] = true;
                group.push(adjacent);
            }
        }
        cursor += 1;
    }
    group.sort_unstable();
    group.into_iter().map(|i| point(size, i)).collect()
}

fn result_text(lead: f64, komi: f64) -> String {
    if lead == 0.0 {
        return "0".into();
    }
    let komi = komi.to_string();
    let precision = komi.split_once('.').map_or(0, |(_, text)| text.len());
    let amount = format!("{:.*}", precision, lead.abs());
    let amount = if precision == 0 {
        amount.as_str()
    } else {
        amount.trim_end_matches('0').trim_end_matches('.')
    };
    format!("{}+{amount}", if lead > 0.0 { "B" } else { "W" })
}

fn color_index(color: Color) -> usize {
    match color {
        Color::Black => 0,
        Color::White => 1,
    }
}
fn index(size: BoardSize, point: Point) -> usize {
    usize::from(point.y) * usize::from(size.width) + usize::from(point.x)
}
fn point(size: BoardSize, index: usize) -> Point {
    Point {
        x: (index % usize::from(size.width)) as u8,
        y: (index / usize::from(size.width)) as u8,
    }
}
fn neighbors(size: BoardSize, index: usize) -> impl Iterator<Item = usize> {
    let width = usize::from(size.width);
    let x = index % width;
    let y = index / width;
    [
        (x > 0).then(|| index - 1),
        (x + 1 < width).then_some(index + 1),
        (y > 0).then(|| index - width),
        (y + 1 < usize::from(size.height)).then_some(index + width),
    ]
    .into_iter()
    .flatten()
}

#[cfg(test)]
mod tests;
