use go_core::{BoardSize, Color, Move, Point};

use crate::{MAX_COMMAND_BYTES, ProtocolError};

/// Arguments are tokens. Use `parse` for an advanced user command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub id: Option<u32>,
    pub name: String,
    pub arguments: Vec<String>,
}

impl Command {
    pub fn parse(line: &str) -> Result<Self, ProtocolError> {
        if line.len() > MAX_COMMAND_BYTES {
            return Err(ProtocolError::TooLarge);
        }
        if line.chars().any(|c| c.is_control() && c != '\t') {
            return Err(ProtocolError::InvalidCommand(
                "embedded control character".into(),
            ));
        }
        let mut tokens = line.split_ascii_whitespace();
        let first = tokens
            .next()
            .ok_or_else(|| ProtocolError::InvalidCommand("empty command".into()))?;
        let (id, name) = if first.bytes().all(|b| b.is_ascii_digit()) {
            let id = first
                .parse()
                .map_err(|_| ProtocolError::InvalidCommand("id out of range".into()))?;
            (
                Some(id),
                tokens
                    .next()
                    .ok_or_else(|| ProtocolError::InvalidCommand("missing name".into()))?,
            )
        } else {
            (None, first)
        };
        let command = Self {
            id,
            name: name.into(),
            arguments: tokens.map(str::to_owned).collect(),
        };
        command.validate()?;
        Ok(command)
    }

    pub fn encode(&self) -> Result<String, ProtocolError> {
        self.validate()?;
        let mut result = self.id.map_or_else(String::new, |id| format!("{id} "));
        result.push_str(&self.name);
        for argument in &self.arguments {
            result.push(' ');
            result.push_str(argument);
        }
        if result.len() > MAX_COMMAND_BYTES {
            return Err(ProtocolError::TooLarge);
        }
        Ok(result)
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        if self.name.is_empty()
            || !self
                .name
                .bytes()
                .all(|b| b.is_ascii_alphabetic() || b == b'-' || b == b'_')
        {
            return Err(ProtocolError::InvalidCommand("invalid command name".into()));
        }
        if self.arguments.iter().any(|arg| {
            arg.is_empty()
                || arg
                    .chars()
                    .any(|c| c.is_whitespace() || c.is_control() || c == '#')
        }) {
            return Err(ProtocolError::InvalidCommand(
                "invalid argument token".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum StandardCommand {
    ProtocolVersion,
    Name,
    Version,
    KnownCommand(String),
    ListCommands,
    BoardSize(u8),
    ClearBoard,
    Komi(f64),
    FixedHandicap(u8),
    PlaceFreeHandicap(u8),
    SetFreeHandicap {
        points: Vec<Point>,
        size: BoardSize,
    },
    Play {
        mv: Move,
        size: BoardSize,
    },
    Genmove(Color),
    Undo,
    TimeSettings {
        main: u32,
        period: u32,
        stones: u32,
    },
    TimeLeft {
        color: Color,
        seconds: u32,
        stones: u32,
    },
    FinalScore,
    Quit,
    /// Caller must first check the negotiated command list.
    KataSetRules(String),
    /// Caller must first check the negotiated command list.
    KataTimeSettings(Vec<String>),
}

impl StandardCommand {
    pub fn encode(&self, id: Option<u32>) -> Result<String, ProtocolError> {
        let line = match self {
            Self::ProtocolVersion => "protocol_version".into(),
            Self::Name => "name".into(),
            Self::Version => "version".into(),
            Self::KnownCommand(name) => format!("known_command {name}"),
            Self::ListCommands => "list_commands".into(),
            Self::BoardSize(size) => {
                BoardSize::new(*size, *size)
                    .map_err(|e| ProtocolError::InvalidCommand(e.to_string()))?;
                format!("boardsize {size}")
            }
            Self::ClearBoard => "clear_board".into(),
            Self::Komi(komi) => format!("komi {}", number(*komi)?),
            Self::FixedHandicap(n) => format!("fixed_handicap {n}"),
            Self::PlaceFreeHandicap(n) => format!("place_free_handicap {n}"),
            Self::SetFreeHandicap { points, size } => {
                if points.is_empty() {
                    return Err(ProtocolError::InvalidCommand("empty handicap".into()));
                }
                let points = points
                    .iter()
                    .map(|point| {
                        point
                            .to_gtp(*size)
                            .map_err(|e| ProtocolError::Coordinate(e.to_string()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                format!("set_free_handicap {}", points.join(" "))
            }
            Self::Play { mv, size } => format!(
                "play {} {}",
                color(mv.color()),
                mv.to_gtp(*size)
                    .map_err(|e| ProtocolError::Coordinate(e.to_string()))?
            ),
            Self::Genmove(side) => format!("genmove {}", color(*side)),
            Self::Undo => "undo".into(),
            Self::TimeSettings {
                main,
                period,
                stones,
            } => format!(
                "time_settings {} {} {}",
                integer(*main)?,
                integer(*period)?,
                integer(*stones)?
            ),
            Self::TimeLeft {
                color: side,
                seconds,
                stones,
            } => format!(
                "time_left {} {} {}",
                color(*side),
                integer(*seconds)?,
                integer(*stones)?
            ),
            Self::FinalScore => "final_score".into(),
            Self::Quit => "quit".into(),
            Self::KataSetRules(rules) => format!("kata-set-rules {rules}"),
            Self::KataTimeSettings(args) => format!("kata-time_settings {}", args.join(" ")),
        };
        let mut command = Command::parse(&line)?;
        command.id = id;
        command.encode()
    }
}

fn color(side: Color) -> &'static str {
    match side {
        Color::Black => "b",
        Color::White => "w",
    }
}

fn number(value: f64) -> Result<String, ProtocolError> {
    if !value.is_finite() {
        Err(ProtocolError::InvalidCommand(
            "invalid numeric argument".into(),
        ))
    } else {
        Ok(value.to_string())
    }
}

// GTP 2 int is nonnegative and at most 2^31 - 1.
fn integer(value: u32) -> Result<u32, ProtocolError> {
    if value > i32::MAX as u32 {
        Err(ProtocolError::InvalidCommand(
            "GTP integer out of range".into(),
        ))
    } else {
        Ok(value)
    }
}
