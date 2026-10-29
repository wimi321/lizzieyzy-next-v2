use thiserror::Error;

/// A player's stone color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    Black,
    White,
}

impl Color {
    pub const fn opponent(self) -> Self {
        match self {
            Self::Black => Self::White,
            Self::White => Self::Black,
        }
    }
}

/// Zero-based board coordinates, with the origin at the top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: u8,
    pub y: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Move {
    Play(Color, Point),
    Pass(Color),
}

impl Move {
    pub const fn color(self) -> Color {
        match self {
            Self::Play(color, _) | Self::Pass(color) => color,
        }
    }
}

/// Board dimensions. Consumers validate public fields at their API boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoardSize {
    pub width: u8,
    pub height: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("invalid board size {width}x{height}: each dimension must be in 2..=25")]
pub struct BoardSizeError {
    pub width: u8,
    pub height: u8,
}

impl BoardSize {
    pub fn new(width: u8, height: u8) -> Result<Self, BoardSizeError> {
        Self { width, height }.validate()
    }

    pub fn validate(self) -> Result<Self, BoardSizeError> {
        if (2..=25).contains(&self.width) && (2..=25).contains(&self.height) {
            Ok(self)
        } else {
            Err(BoardSizeError {
                width: self.width,
                height: self.height,
            })
        }
    }

    pub const fn contains(self, point: Point) -> bool {
        point.x < self.width && point.y < self.height
    }

    pub fn area(self) -> usize {
        usize::from(self.width) * usize::from(self.height)
    }
}

#[cfg(test)]
mod tests;
