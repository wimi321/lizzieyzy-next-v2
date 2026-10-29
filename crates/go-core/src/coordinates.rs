use thiserror::Error;

use crate::{BoardSize, BoardSizeError, Color, Move, Point};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoordinateError {
    #[error(transparent)]
    InvalidBoardSize(#[from] BoardSizeError),
    #[error("invalid GTP coordinate {0:?}: expected a column A..Z except I and a row 1..25")]
    InvalidGtp(String),
    #[error("invalid SGF coordinate {0:?}: expected two lowercase letters a..y")]
    InvalidSgf(String),
    #[error("point {point:?} is outside board {size:?}")]
    OutOfBounds { point: Point, size: BoardSize },
    #[error("pass has no board point; use Move to represent pass")]
    PassHasNoPoint,
}

impl Point {
    pub fn to_gtp(self, size: BoardSize) -> Result<String, CoordinateError> {
        validate_point(self, size)?;
        let column = b'A' + self.x + u8::from(self.x >= 8);
        Ok(format!("{}{}", char::from(column), size.height - self.y))
    }

    pub fn from_gtp(value: &str, size: BoardSize) -> Result<Self, CoordinateError> {
        match Move::from_gtp(Color::Black, value, size)? {
            Move::Play(_, point) => Ok(point),
            Move::Pass(_) => Err(CoordinateError::PassHasNoPoint),
        }
    }

    pub fn to_sgf(self, size: BoardSize) -> Result<String, CoordinateError> {
        validate_point(self, size)?;
        Ok(format!(
            "{}{}",
            char::from(b'a' + self.x),
            char::from(b'a' + self.y)
        ))
    }

    pub fn from_sgf(value: &str, size: BoardSize) -> Result<Self, CoordinateError> {
        match Move::from_sgf(Color::Black, value, size)? {
            Move::Play(_, point) => Ok(point),
            Move::Pass(_) => Err(CoordinateError::PassHasNoPoint),
        }
    }
}

impl Move {
    pub fn to_gtp(self, size: BoardSize) -> Result<String, CoordinateError> {
        size.validate()?;
        match self {
            Self::Play(_, point) => point.to_gtp(size),
            Self::Pass(_) => Ok(String::from("pass")),
        }
    }

    /// GTP columns and `pass` are case-insensitive; output is canonical uppercase.
    pub fn from_gtp(color: Color, value: &str, size: BoardSize) -> Result<Self, CoordinateError> {
        size.validate()?;
        if value.eq_ignore_ascii_case("pass") {
            return Ok(Self::Pass(color));
        }
        let bytes = value.as_bytes();
        let invalid = || CoordinateError::InvalidGtp(value.to_owned());
        if !(2..=3).contains(&bytes.len()) {
            return Err(invalid());
        }
        let column = bytes[0].to_ascii_uppercase();
        if !column.is_ascii_uppercase()
            || column == b'I'
            || bytes[1] == b'0'
            || !bytes[1..].iter().all(u8::is_ascii_digit)
        {
            return Err(invalid());
        }
        let row: u8 = value[1..].parse().map_err(|_| invalid())?;
        let x = column - b'A' - u8::from(column > b'I');
        if row > size.height || x >= size.width {
            return Err(invalid());
        }
        Ok(Self::Play(
            color,
            Point {
                x,
                y: size.height - row,
            },
        ))
    }

    pub fn to_sgf(self, size: BoardSize) -> Result<String, CoordinateError> {
        size.validate()?;
        match self {
            Self::Play(_, point) => point.to_sgf(size),
            Self::Pass(_) => Ok(String::new()),
        }
    }

    /// Empty values are pass. Legacy `tt` is pass only when both dimensions <= 19.
    pub fn from_sgf(color: Color, value: &str, size: BoardSize) -> Result<Self, CoordinateError> {
        size.validate()?;
        if value.is_empty() || (value == "tt" && size.width <= 19 && size.height <= 19) {
            return Ok(Self::Pass(color));
        }
        let bytes = value.as_bytes();
        if bytes.len() != 2 || !bytes.iter().all(|byte| (b'a'..=b'y').contains(byte)) {
            return Err(CoordinateError::InvalidSgf(value.to_owned()));
        }
        let point = Point {
            x: bytes[0] - b'a',
            y: bytes[1] - b'a',
        };
        validate_point(point, size)?;
        Ok(Self::Play(color, point))
    }
}

fn validate_point(point: Point, size: BoardSize) -> Result<(), CoordinateError> {
    size.validate()?;
    if size.contains(point) {
        Ok(())
    } else {
        Err(CoordinateError::OutOfBounds { point, size })
    }
}

#[cfg(test)]
mod tests;
