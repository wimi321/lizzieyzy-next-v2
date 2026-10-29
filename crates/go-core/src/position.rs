use std::collections::HashSet;

use thiserror::Error;

use crate::{BoardSize, BoardSizeError, Color, KoRule, Move, Point, Rules, zobrist};

const MAX_POINTS: usize = 25 * 25;

/// A rejected move never changes the position, its turn, or its repetition history.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum IllegalMove {
    #[error("point {point:?} is outside board {size:?}")]
    OutOfBounds { point: Point, size: BoardSize },
    #[error("point {point:?} is already occupied")]
    Occupied { point: Point },
    #[error("move at {point:?} leaves its own group without liberties")]
    Suicide { point: Point },
    #[error("move at {point:?} repeats a position forbidden by {rule:?}")]
    Ko { point: Point, rule: KoRule },
}

/// Setup is a new history root, not a sequence of capturing moves.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum SetupError {
    #[error(transparent)]
    InvalidSize(#[from] BoardSizeError),
    #[error("setup point {point:?} is outside board {size:?}")]
    OutOfBounds { point: Point, size: BoardSize },
    #[error("setup contains point {point:?} more than once")]
    DuplicatePoint { point: Point },
    #[error("setup group at {point:?} has no liberties")]
    NoLiberties { point: Point },
}

/// Removed points are ordered row by row, left to right.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayOutcome {
    /// Opposing stones removed by this move.
    pub captured: Vec<Point>,
    /// Friendly stones removed by legal multi-stone suicide, including the played stone.
    pub self_captured: Vec<Point>,
}

/// Pure, synchronous board state with private mutation and repetition history.
///
/// Moves carry their own color, including for SGF records with consecutive moves
/// of one color. The next player becomes that color's opponent. Passes always
/// succeed, clear simple ko, and enter the resulting state into superko history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    size: BoardSize,
    rules: Rules,
    stones: Vec<Option<Color>>,
    to_play: Color,
    board_hash: u64,
    simple_ko: Option<(Color, Point)>,
    history: HashSet<u64>,
}

impl Position {
    /// Construct an empty board with Black to play.
    pub fn new(size: BoardSize, rules: Rules) -> Result<Self, BoardSizeError> {
        let size = size.validate()?;
        let mut position = Self {
            size,
            rules,
            stones: vec![None; size.area()],
            to_play: Color::Black,
            board_hash: zobrist::board_size(size),
            simple_ko: None,
            history: HashSet::new(),
        };
        position.record_history();
        Ok(position)
    }

    /// Construct an initial position without applying captures or alternating turns.
    /// Duplicates and groups without liberties are rejected, even with multi-stone suicide enabled.
    pub fn with_setup(
        size: BoardSize,
        rules: Rules,
        to_play: Color,
        stones: impl IntoIterator<Item = (Color, Point)>,
    ) -> Result<Self, SetupError> {
        let mut position = Self::new(size, rules)?;
        for (color, point) in stones {
            if !size.contains(point) {
                return Err(SetupError::OutOfBounds { point, size });
            }
            let index = position.index(point);
            if position.stones[index].is_some() {
                return Err(SetupError::DuplicatePoint { point });
            }
            position.stones[index] = Some(color);
            position.board_hash ^= zobrist::stone(point, color);
        }
        let mut visited = [false; MAX_POINTS];
        for index in 0..position.stones.len() {
            if visited[index] || position.stones[index].is_none() {
                continue;
            }
            let group = group_at(&position.stones, size, index);
            if group.liberties == 0 {
                return Err(SetupError::NoLiberties {
                    point: position.point(index),
                });
            }
            for member in group.stones {
                visited[member] = true;
            }
        }
        position.to_play = to_play;
        position.history.clear();
        position.record_history();
        Ok(position)
    }

    pub fn size(&self) -> BoardSize {
        self.size
    }

    pub fn rules(&self) -> Rules {
        self.rules
    }

    pub fn to_play(&self) -> Color {
        self.to_play
    }

    /// Row-major storage, starting at the top-left corner.
    pub fn stones(&self) -> &[Option<Color>] {
        &self.stones
    }

    /// Returns `None` for both empty and out-of-bounds points.
    pub fn stone_at(&self, point: Point) -> Option<Color> {
        if self.size.contains(point) {
            self.stones[self.index(point)]
        } else {
            None
        }
    }

    /// Number of distinct liberties of the entire group at a point.
    /// Empty or out-of-bounds points return `None`.
    pub fn liberties_at(&self, point: Point) -> Option<usize> {
        self.stone_at(point)?;
        Some(group_at(&self.stones, self.size, self.index(point)).liberties)
    }

    /// Fixed-seed Zobrist hash of board size, stones, and next player.
    /// Rules and history are deliberately not included; this is not an analysis cache key.
    pub fn hash(&self) -> u64 {
        self.board_hash ^ zobrist::turn(self.to_play)
    }

    /// Independently scan the board to recompute the same hash from its contents.
    pub fn recompute_hash(&self) -> u64 {
        let mut hash = zobrist::board_size(self.size) ^ zobrist::turn(self.to_play);
        for (index, stone) in self.stones.iter().enumerate() {
            if let Some(color) = stone {
                hash ^= zobrist::stone(self.point(index), *color);
            }
        }
        hash
    }

    /// Apply capture, suicide and repetition rules. Errors leave all state unchanged.
    pub fn play(&mut self, mv: Move) -> Result<PlayOutcome, IllegalMove> {
        self.play_impl(mv, true)
    }

    /// SGF replay: skip only ko checks, retaining all other move checks and captures.
    /// Accepted moves still update history for subsequent fully checked moves.
    pub fn play_unchecked_ko(&mut self, mv: Move) -> Result<PlayOutcome, IllegalMove> {
        self.play_impl(mv, false)
    }

    fn play_impl(&mut self, mv: Move, check_ko: bool) -> Result<PlayOutcome, IllegalMove> {
        let Move::Play(color, point) = mv else {
            self.to_play = mv.color().opponent();
            self.simple_ko = None;
            self.record_history();
            return Ok(PlayOutcome::default());
        };
        if !self.size.contains(point) {
            return Err(IllegalMove::OutOfBounds {
                point,
                size: self.size,
            });
        }
        let index = self.index(point);
        if self.stones[index].is_some() {
            return Err(IllegalMove::Occupied { point });
        }
        if check_ko && self.rules.ko == KoRule::Simple && self.simple_ko == Some((color, point)) {
            return Err(IllegalMove::Ko {
                point,
                rule: self.rules.ko,
            });
        }

        // Copy only the small board, never the growing history. Commit after all checks.
        let mut stones = self.stones.clone();
        let mut board_hash = self.board_hash ^ zobrist::stone(point, color);
        stones[index] = Some(color);
        let mut outcome = PlayOutcome::default();
        let mut visited = [false; MAX_POINTS];
        for neighbor in neighbors(self.size, index) {
            if visited[neighbor] || stones[neighbor] != Some(color.opponent()) {
                continue;
            }
            let group = group_at(&stones, self.size, neighbor);
            for member in group.stones {
                visited[member] = true;
                if group.liberties == 0 {
                    let captured = self.point(member);
                    stones[member] = None;
                    board_hash ^= zobrist::stone(captured, color.opponent());
                    outcome.captured.push(captured);
                }
            }
        }
        let own_group = group_at(&stones, self.size, index);
        let mut simple_ko = None;
        if own_group.liberties == 0 {
            if own_group.stones.len() == 1 || !self.rules.allow_multi_stone_suicide {
                return Err(IllegalMove::Suicide { point });
            }
            for member in own_group.stones {
                let removed = self.point(member);
                stones[member] = None;
                board_hash ^= zobrist::stone(removed, color);
                outcome.self_captured.push(removed);
            }
        } else if outcome.captured.len() == 1
            && own_group.stones.len() == 1
            && own_group.liberties == 1
        {
            simple_ko = Some((color.opponent(), outcome.captured[0]));
        }

        let next_player = color.opponent();
        if check_ko
            && self.rules.ko != KoRule::Simple
            && self
                .history
                .contains(&self.history_key(board_hash, next_player))
        {
            return Err(IllegalMove::Ko {
                point,
                rule: self.rules.ko,
            });
        }

        outcome.captured.sort_unstable_by_key(|p| (p.y, p.x));
        outcome.self_captured.sort_unstable_by_key(|p| (p.y, p.x));
        self.stones = stones;
        self.board_hash = board_hash;
        self.to_play = next_player;
        self.simple_ko = simple_ko;
        self.record_history();
        Ok(outcome)
    }

    fn index(&self, point: Point) -> usize {
        usize::from(point.y) * usize::from(self.size.width) + usize::from(point.x)
    }

    fn point(&self, index: usize) -> Point {
        Point {
            x: (index % usize::from(self.size.width)) as u8,
            y: (index / usize::from(self.size.width)) as u8,
        }
    }

    fn history_key(&self, board_hash: u64, to_play: Color) -> u64 {
        match self.rules.ko {
            KoRule::Simple | KoRule::PositionalSuperko => board_hash,
            KoRule::SituationalSuperko => board_hash ^ zobrist::turn(to_play),
        }
    }

    fn record_history(&mut self) {
        self.history
            .insert(self.history_key(self.board_hash, self.to_play));
    }
}

struct Group {
    stones: Vec<usize>,
    liberties: usize,
}

// Called only for occupied, in-bounds points on a validated board.
fn group_at(stones: &[Option<Color>], size: BoardSize, start: usize) -> Group {
    let color = stones[start];
    let mut visited = [false; MAX_POINTS];
    let mut liberties = [false; MAX_POINTS];
    let mut group = Group {
        stones: vec![start],
        liberties: 0,
    };
    visited[start] = true;
    let mut cursor = 0;
    while cursor < group.stones.len() {
        for neighbor in neighbors(size, group.stones[cursor]) {
            if stones[neighbor].is_none() {
                if !liberties[neighbor] {
                    liberties[neighbor] = true;
                    group.liberties += 1;
                }
            } else if stones[neighbor] == color && !visited[neighbor] {
                visited[neighbor] = true;
                group.stones.push(neighbor);
            }
        }
        cursor += 1;
    }
    group
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
