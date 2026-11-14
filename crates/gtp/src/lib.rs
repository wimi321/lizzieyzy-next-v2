//! Pure, bounded GTP version 2 command and response codec.

mod command;
mod response;

pub use command::{Command, StandardCommand};
use go_core::{BoardSize, Point};
pub use response::{Decoder, Response, parse_response};

pub const MAX_COMMAND_BYTES: usize = 8192;
pub const MAX_RESPONSE_BYTES: usize = 65536;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("invalid GTP command: {0}")]
    InvalidCommand(String),
    #[error("invalid GTP response: {0}")]
    InvalidResponse(String),
    #[error("incomplete GTP response")]
    IncompleteResponse,
    #[error("GTP message exceeds its size limit")]
    TooLarge,
    #[error("invalid GTP coordinate: {0}")]
    Coordinate(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Genmove {
    Play(Point),
    Pass,
    Resign,
}

pub fn parse_genmove(text: &str, size: BoardSize) -> Result<Genmove, ProtocolError> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("pass") {
        Ok(Genmove::Pass)
    } else if text.eq_ignore_ascii_case("resign") {
        Ok(Genmove::Resign)
    } else {
        Point::from_gtp(text, size)
            .map(Genmove::Play)
            .map_err(|error| ProtocolError::Coordinate(error.to_string()))
    }
}

#[cfg(test)]
mod tests;
