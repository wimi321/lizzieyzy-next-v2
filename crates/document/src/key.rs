use go_core::{Color, Move, RulePreset};

use crate::GamePosition;

/// Stable BLAKE3 digest of the inputs that identify a document position.
///
/// The v1 encoding is independent of Rust layouts, enum discriminants, platform
/// endianness, and `Hash` implementations. Keep its byte encoding unchanged when
/// refactoring; changing it requires a new cache-key format version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnalysisKey(pub [u8; 32]);

impl AnalysisKey {
    pub fn for_position(position: &GamePosition) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"lizzieyzy-next2:analysis-key:v1\0");
        let size = position.board.size();
        hasher.update(&[
            size.width,
            size.height,
            color_tag(position.initial_player),
            color_tag(position.board.to_play()),
        ]);

        // SGF setup property order does not affect the position. Canonicalize it
        // here as well as in replay, so callers constructing snapshots get the
        // same key for the same setup.
        let mut stones = position.initial_stones.clone();
        stones.sort_unstable_by_key(|(color, point)| (point.y, point.x, color_tag(*color)));
        hasher.update(&(stones.len() as u64).to_le_bytes());
        for (color, point) in stones {
            hasher.update(&[color_tag(color), point.x, point.y]);
        }
        hash_moves(&mut hasher, &position.moves);
        hash_moves(&mut hasher, &position.analysis_moves);
        hasher.update(&[rules_tag(position.rules)]);

        // IEEE-754 negative zero denotes the same komi as positive zero.
        let komi = if position.komi == 0.0 {
            0.0
        } else {
            position.komi
        };
        hasher.update(&komi.to_bits().to_le_bytes());
        Self(*hasher.finalize().as_bytes())
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl GamePosition {
    pub fn analysis_key(&self) -> AnalysisKey {
        AnalysisKey::for_position(self)
    }
}

fn hash_moves(hasher: &mut blake3::Hasher, moves: &[Move]) {
    hasher.update(&(moves.len() as u64).to_le_bytes());
    for mv in moves {
        match mv {
            Move::Pass(color) => hasher.update(&[0, color_tag(*color)]),
            Move::Play(color, point) => hasher.update(&[1, color_tag(*color), point.x, point.y]),
        };
    }
}

const fn color_tag(color: Color) -> u8 {
    match color {
        Color::Black => 0,
        Color::White => 1,
    }
}

const fn rules_tag(rules: RulePreset) -> u8 {
    match rules {
        RulePreset::Chinese => 0,
        RulePreset::Japanese => 1,
        RulePreset::Korean => 2,
        RulePreset::Aga => 3,
        RulePreset::TrompTaylor => 4,
        RulePreset::NewZealand => 5,
    }
}

#[cfg(test)]
mod tests;
