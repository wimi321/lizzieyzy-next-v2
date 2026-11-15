//! Bounded, rule-aware reading of a single ladder. No engine or board mutation.

use crate::{Color, Move, Point, Position};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LadderOutcome {
    Captured,
    Escaped,
    NotLadder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LadderReading {
    pub outcome: LadderOutcome,
    /// A legal continuation. Capture lines stop at the final atari, before
    /// the losing extension or final capture; escape lines include the escape.
    pub moves: Vec<Move>,
}

impl LadderReading {
    fn terminal(outcome: LadderOutcome) -> Self {
        Self {
            outcome,
            moves: Vec::new(),
        }
    }

    fn prepend(mut self, mv: Move) -> Self {
        if self.outcome != LadderOutcome::NotLadder {
            self.moves.insert(0, mv);
        }
        self
    }
}

/// Read a uniquely identifiable atari group belonging to `to_play`; otherwise
/// read a unique opposing two-liberty group. Multiple eligible groups, a turn
/// mismatch, repetition, or an exhausted reading budget are conservatively
/// `NotLadder`. An attack must reduce the target to one liberty. Both atari
/// choices and adjacent countercaptures are read, rather than extrapolating a
/// geometric pattern. At most twice the board area in plies and 32 times its
/// area in explored positions are allowed; every published move obeys ko.
pub fn read_ladder(position: &Position, to_play: Color) -> LadderReading {
    if position.to_play() != to_play {
        return LadderReading::terminal(LadderOutcome::NotLadder);
    }
    let mut defending = Vec::new();
    let mut attacking = Vec::new();
    let mut visited = vec![false; position.size().area()];
    for index in 0..visited.len() {
        let point = at(position, index);
        let Some(color) = position.stone_at(point) else {
            continue;
        };
        if visited[index] {
            continue;
        }
        let (stones, liberties) = group(position, point);
        for stone in stones {
            visited[offset(position, stone)] = true;
        }
        if color == to_play && liberties.len() == 1 {
            defending.push(point);
        } else if color != to_play && liberties.len() == 2 {
            attacking.push(point);
        }
    }
    let targets = if defending.is_empty() {
        attacking
    } else {
        defending
    };
    let [target] = targets.as_slice() else {
        return LadderReading::terminal(LadderOutcome::NotLadder);
    };
    let Some(defender) = position.stone_at(*target) else {
        return LadderReading::terminal(LadderOutcome::NotLadder);
    };
    let mut reader = Reader {
        target: *target,
        defender,
        remaining: position.size().area() * 32,
        max_plies: position.size().area() * 2,
        path: Vec::new(),
    };
    reader.read(position)
}

struct Reader {
    target: Point,
    defender: Color,
    remaining: usize,
    max_plies: usize,
    path: Vec<u64>,
}

impl Reader {
    fn read(&mut self, position: &Position) -> LadderReading {
        if self.remaining == 0
            || self.path.len() >= self.max_plies
            || self.path.contains(&position.hash())
        {
            return LadderReading::terminal(LadderOutcome::NotLadder);
        }
        self.remaining -= 1;
        self.path.push(position.hash());
        let result = if position.to_play() == self.defender {
            self.defend(position)
        } else {
            self.attack(position)
        };
        self.path.pop();
        result
    }

    fn defend(&mut self, position: &Position) -> LadderReading {
        let (stones, liberties) = group(position, self.target);
        if liberties.len() != 1 {
            return LadderReading::terminal(LadderOutcome::NotLadder);
        }
        // Capturing an adjacent attacker in atari is a real escape even when
        // simply extending the target would lose. Do not read only its liberty.
        let mut countercaptures = Vec::new();
        for stone in stones {
            for adjacent in neighbors(position, stone) {
                if position.stone_at(adjacent) == Some(self.defender.opponent()) {
                    let (_, attacker_liberties) = group(position, adjacent);
                    if let [liberty] = attacker_liberties.as_slice() {
                        countercaptures.push(*liberty);
                    }
                }
            }
        }
        countercaptures.sort_unstable_by_key(|p| (p.y, p.x));
        countercaptures.dedup();
        for point in countercaptures {
            let mut next = position.clone();
            let mv = Move::Play(self.defender, point);
            if let Ok(outcome) = next.play(mv)
                && !outcome.captured.is_empty()
            {
                return LadderReading::terminal(LadderOutcome::Escaped).prepend(mv);
            }
        }
        let mv = Move::Play(self.defender, liberties[0]);
        let mut next = position.clone();
        let Ok(outcome) = next.play(mv) else {
            return LadderReading::terminal(LadderOutcome::Captured);
        };
        let remaining_liberties = next.liberties_at(self.target).unwrap_or(0);
        if !outcome.captured.is_empty() || remaining_liberties >= 3 {
            return LadderReading::terminal(LadderOutcome::Escaped).prepend(mv);
        }
        if remaining_liberties <= 1 {
            // The existing board is already the final atari. Do not append a
            // futile extension and then leave the document at the wrong ply.
            return LadderReading::terminal(LadderOutcome::Captured);
        }
        self.read(&next).prepend(mv)
    }

    fn attack(&mut self, position: &Position) -> LadderReading {
        let (_, liberties) = group(position, self.target);
        if liberties.len() != 2 {
            return LadderReading::terminal(LadderOutcome::NotLadder);
        }
        let mut escape = None;
        let mut unresolved = false;
        for point in liberties {
            let mv = Move::Play(self.defender.opponent(), point);
            let mut next = position.clone();
            if next.play(mv).is_err() || next.liberties_at(self.target) != Some(1) {
                continue;
            }
            let result = self.read(&next).prepend(mv);
            match result.outcome {
                LadderOutcome::Captured => return result,
                LadderOutcome::Escaped => {
                    if escape.is_none() {
                        escape = Some(result);
                    }
                }
                LadderOutcome::NotLadder => unresolved = true,
            }
        }
        if unresolved {
            LadderReading::terminal(LadderOutcome::NotLadder)
        } else {
            escape.unwrap_or_else(|| LadderReading::terminal(LadderOutcome::NotLadder))
        }
    }
}

fn group(position: &Position, start: Point) -> (Vec<Point>, Vec<Point>) {
    let Some(color) = position.stone_at(start) else {
        return (Vec::new(), Vec::new());
    };
    let mut visited = vec![false; position.size().area()];
    let mut stones = vec![start];
    let mut liberties = Vec::new();
    visited[offset(position, start)] = true;
    let mut cursor = 0;
    while cursor < stones.len() {
        for point in neighbors(position, stones[cursor]) {
            let index = offset(position, point);
            if visited[index] {
                continue;
            }
            if position.stone_at(point).is_none() {
                liberties.push(point);
                visited[index] = true;
            } else if position.stone_at(point) == Some(color) {
                stones.push(point);
                visited[index] = true;
            }
        }
        cursor += 1;
    }
    liberties.sort_unstable_by_key(|p| (p.y, p.x));
    (stones, liberties)
}

fn neighbors(position: &Position, point: Point) -> impl Iterator<Item = Point> {
    let size = position.size();
    [
        point.x.checked_sub(1).map(|x| Point { x, y: point.y }),
        (point.x + 1 < size.width).then_some(Point {
            x: point.x + 1,
            y: point.y,
        }),
        point.y.checked_sub(1).map(|y| Point { x: point.x, y }),
        (point.y + 1 < size.height).then_some(Point {
            x: point.x,
            y: point.y + 1,
        }),
    ]
    .into_iter()
    .flatten()
}

fn offset(position: &Position, point: Point) -> usize {
    usize::from(point.y) * usize::from(position.size().width) + usize::from(point.x)
}

fn at(position: &Position, index: usize) -> Point {
    let width = usize::from(position.size().width);
    Point {
        x: (index % width) as u8,
        y: (index / width) as u8,
    }
}

#[cfg(test)]
mod tests;
