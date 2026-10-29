//! Fixed-seed Zobrist keys. No runtime RNG, global state, or platform hashers.

use crate::{BoardSize, Color, Point};

const SEED: u64 = 0x4c59_5a59_474f_0001;
const MAX_SIDE: usize = 25;
const STONE_KEYS: [[u64; 2]; MAX_SIDE * MAX_SIDE] = stone_keys();

// SplitMix64's output permutation applied to a fixed sequence of seed values.
const fn key(index: u64) -> u64 {
    let mut value = SEED.wrapping_add(index.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

const fn stone_keys() -> [[u64; 2]; MAX_SIDE * MAX_SIDE] {
    let mut keys = [[0; 2]; MAX_SIDE * MAX_SIDE];
    let mut index = 0;
    while index < keys.len() {
        keys[index] = [key(2 * index as u64), key(2 * index as u64 + 1)];
        index += 1;
    }
    keys
}

pub(crate) fn stone(point: Point, color: Color) -> u64 {
    STONE_KEYS[usize::from(point.y) * MAX_SIDE + usize::from(point.x)][match color {
        Color::Black => 0,
        Color::White => 1,
    }]
}

pub(crate) const fn turn(color: Color) -> u64 {
    key(match color {
        Color::Black => 1250,
        Color::White => 1251,
    })
}

pub(crate) const fn board_size(size: BoardSize) -> u64 {
    key(1252 + size.width as u64 * 26 + size.height as u64)
}
