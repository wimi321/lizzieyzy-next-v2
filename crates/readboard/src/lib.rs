//! Synchronous UTF-8 line protocol for qiyi71w/readboard; no process or I/O ownership.

use go_core::{BoardSize, Color, Point};
use thiserror::Error;

pub const MAX_LINE_BYTES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Black,
    White,
    LastBlack,
    LastWhite,
}

impl Cell {
    pub fn color(self) -> Option<Color> {
        match self {
            Self::Empty => None,
            Self::Black | Self::LastBlack => Some(Color::Black),
            Self::White | Self::LastWhite => Some(Color::White),
        }
    }

    pub fn is_last_move(self) -> bool {
        matches!(self, Self::LastBlack | Self::LastWhite)
    }

    pub fn code(self) -> u8 {
        match self {
            Self::Empty => 0,
            Self::Black => 1,
            Self::White => 2,
            Self::LastBlack => 3,
            Self::LastWhite => 4,
        }
    }

    fn parse(value: &str) -> Result<Self, ProtocolError> {
        match value {
            "0" => Ok(Self::Empty),
            "1" => Ok(Self::Black),
            "2" => Ok(Self::White),
            "3" => Ok(Self::LastBlack),
            "4" => Ok(Self::LastWhite),
            _ => Err(ProtocolError::InvalidRow),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteContext {
    pub platform: Option<String>,
    pub room_token: Option<String>,
    pub live_title_move: Option<u32>,
    pub record_current_move: Option<u32>,
    pub record_total_move: Option<u32>,
    pub record_at_end: Option<bool>,
    pub record_title_fingerprint: Option<String>,
    pub last_move_source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub size: BoardSize,
    /// Row-major from the top-left, including the helper's last-move marker.
    pub cells: Vec<Cell>,
    pub fox_move_number: Option<u32>,
    pub force_rebuild: bool,
    pub context: RemoteContext,
}

impl Snapshot {
    pub fn last_move(&self) -> Option<(Color, Point)> {
        let mut marked = self
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_last_move());
        let (index, cell) = marked.next()?;
        if marked.next().is_some() || self.size.width == 0 {
            return None;
        }
        Some((
            cell.color()?,
            Point {
                x: (index % usize::from(self.size.width)) as u8,
                y: (index / usize::from(self.size.width)) as u8,
            },
        ))
    }

    pub fn stones(&self) -> Vec<Option<Color>> {
        self.cells.iter().map(|cell| cell.color()).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Ready,
    Version(u32),
    Started(BoardSize),
    Snapshot(Snapshot),
    Sync,
    Stopped,
    Ended,
    ClearBoard,
    ExchangeTurn,
    PauseAnalysis,
    ResumeAnalysis,
    Error(String),
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("readboard line exceeds the size limit or contains embedded line endings")]
    InvalidLine,
    #[error("readboard board dimensions must be between 2 and 25")]
    InvalidSize,
    #[error("readboard snapshot row must contain exactly one code (0..4) per column")]
    InvalidRow,
    #[error("readboard frame is incomplete, contains excess rows, or has multiple last moves")]
    InvalidFrame,
    #[error("invalid readboard metadata: {0}")]
    InvalidMetadata(&'static str),
}

/// Owns only a bounded partial frame. A bad row poisons the frame until end/clear/start.
#[derive(Debug)]
pub struct Decoder {
    size: BoardSize,
    cells: Vec<Cell>,
    invalid: bool,
    fox_move_number: Option<u32>,
    force_rebuild: bool,
    context: RemoteContext,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            size: BoardSize {
                width: 19,
                height: 19,
            },
            cells: Vec::new(),
            invalid: false,
            fox_move_number: None,
            force_rebuild: false,
            context: RemoteContext::default(),
        }
    }
}

impl Decoder {
    pub fn push_line(&mut self, raw: &str) -> Result<Option<Event>, ProtocolError> {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.len() > MAX_LINE_BYTES || line.contains(['\r', '\n', '\0']) {
            self.invalid = true;
            return Err(ProtocolError::InvalidLine);
        }
        let result = self.decode(line);
        if result.is_err() && line != "end" {
            self.invalid = true;
        }
        result
    }

    fn decode(&mut self, line: &str) -> Result<Option<Event>, ProtocolError> {
        if let Some(row) = line.strip_prefix("re=") {
            if self.invalid {
                return Err(ProtocolError::InvalidFrame);
            }
            let values = row
                .split(',')
                .map(Cell::parse)
                .collect::<Result<Vec<_>, _>>()?;
            if values.len() != usize::from(self.size.width) {
                return Err(ProtocolError::InvalidRow);
            }
            if self.cells.len() + values.len() > self.area() {
                return Err(ProtocolError::InvalidFrame);
            }
            self.cells.extend(values);
            return Ok(None);
        }
        if line == "end" {
            let invalid = self.invalid
                || self.cells.len() != self.area()
                || self.cells.iter().filter(|c| c.is_last_move()).count() > 1;
            let snapshot = Snapshot {
                size: self.size,
                cells: std::mem::take(&mut self.cells),
                fox_move_number: self.fox_move_number,
                force_rebuild: self.force_rebuild,
                context: std::mem::take(&mut self.context),
            };
            self.reset_frame();
            return if invalid {
                Err(ProtocolError::InvalidFrame)
            } else {
                Ok(Some(Event::Snapshot(snapshot)))
            };
        }
        if line == "start" || line.starts_with("start ") {
            let fields: Vec<_> = line.split_ascii_whitespace().collect();
            let size = match fields.as_slice() {
                [_] => self.size,
                [_, width, height] | [_, width, height, _] => {
                    let width = width.parse().map_err(|_| ProtocolError::InvalidSize)?;
                    let height = height.parse().map_err(|_| ProtocolError::InvalidSize)?;
                    BoardSize::new(width, height).map_err(|_| ProtocolError::InvalidSize)?
                }
                _ => return Err(ProtocolError::InvalidSize),
            };
            self.size = size;
            self.reset_frame();
            return Ok(Some(Event::Started(size)));
        }
        if let Some(value) = line.strip_prefix("version ") {
            return Ok(Some(Event::Version(parse_number(value, "version")?)));
        }
        if let Some(value) = line.strip_prefix("foxMoveNumber ") {
            self.fox_move_number = Some(parse_number(value, "foxMoveNumber")?);
        } else if let Some(value) = line.strip_prefix("liveTitleMove ") {
            self.context.live_title_move = Some(parse_number(value, "liveTitleMove")?);
        } else if let Some(value) = line.strip_prefix("recordCurrentMove ") {
            self.context.record_current_move = Some(parse_number(value, "recordCurrentMove")?);
        } else if let Some(value) = line.strip_prefix("recordTotalMove ") {
            self.context.record_total_move = Some(parse_number(value, "recordTotalMove")?);
        } else if let Some(value) = line.strip_prefix("recordAtEnd ") {
            self.context.record_at_end = Some(match value {
                "0" => false,
                "1" => true,
                _ => return Err(ProtocolError::InvalidMetadata("recordAtEnd")),
            });
        } else if let Some(value) = line.strip_prefix("syncPlatform ") {
            self.context.platform = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("roomToken ") {
            self.context.room_token = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("recordTitleFingerprint ") {
            self.context.record_title_fingerprint = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("lastMoveSource ") {
            self.context.last_move_source = Some(value.to_owned());
        } else if line.starts_with("error") {
            return Ok(Some(Event::Error(line.to_owned())));
        }
        Ok(match line {
            "ready" => Some(Event::Ready),
            "sync" => Some(Event::Sync),
            "stopsync" => {
                self.reset_frame();
                Some(Event::Stopped)
            }
            "endsync" => {
                self.reset_frame();
                Some(Event::Ended)
            }
            "clearBoard" => {
                self.reset_frame();
                Some(Event::ClearBoard)
            }
            "pass" => Some(Event::ExchangeTurn),
            "noponder" => Some(Event::PauseAnalysis),
            "resumeponder" => Some(Event::ResumeAnalysis),
            "clear" => {
                self.reset_frame();
                None
            }
            "forceRebuild" => {
                self.force_rebuild = true;
                None
            }
            _ => None, // Forward-compatible helper capabilities and UI-only messages.
        })
    }

    fn area(&self) -> usize {
        usize::from(self.size.width) * usize::from(self.size.height)
    }

    fn reset_frame(&mut self) {
        self.cells.clear();
        self.invalid = false;
        self.fox_move_number = None;
        self.force_rebuild = false;
        self.context = RemoteContext::default();
    }
}

fn parse_number(value: &str, field: &'static str) -> Result<u32, ProtocolError> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ProtocolError::InvalidMetadata(field));
    }
    value
        .parse()
        .map_err(|_| ProtocolError::InvalidMetadata(field))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Version,
    AnalysisState { running: bool },
    Quit,
}

impl Command {
    /// Includes the terminating newline; never interpolates untrusted text.
    pub fn encode(self) -> &'static str {
        match self {
            Self::Version => "version\n",
            Self::AnalysisState { running: true } => "analysisState running\n",
            Self::AnalysisState { running: false } => "analysisState paused\n",
            Self::Quit => "quit\n",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    Chinese,
    English,
    Japanese,
    Korean,
}

/// Seven legacy positional arguments, preserving each blank autoplay argument.
/// Pipe transport is zero; no TCP listener or automatic move placement is needed.
pub fn launch_arguments(language: Language) -> Vec<String> {
    let language = match language {
        Language::Chinese => "cn",
        Language::English => "en",
        Language::Japanese => "jp",
        Language::Korean => "kr",
    };
    ["yzy", " ", " ", " ", "0", language, "-1"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// Produces the same comma-separated row framing as the upstream snapshot emitter.
pub fn encode_snapshot(snapshot: &Snapshot) -> Result<String, ProtocolError> {
    snapshot
        .size
        .validate()
        .map_err(|_| ProtocolError::InvalidSize)?;
    if snapshot.cells.len() != usize::from(snapshot.size.width) * usize::from(snapshot.size.height)
        || snapshot.cells.iter().filter(|c| c.is_last_move()).count() > 1
    {
        return Err(ProtocolError::InvalidFrame);
    }
    let mut output = format!("start {} {}\n", snapshot.size.width, snapshot.size.height);
    for (key, value) in [
        ("syncPlatform", &snapshot.context.platform),
        ("roomToken", &snapshot.context.room_token),
        (
            "recordTitleFingerprint",
            &snapshot.context.record_title_fingerprint,
        ),
        ("lastMoveSource", &snapshot.context.last_move_source),
    ] {
        if let Some(value) = value {
            if value.contains(['\r', '\n', '\0']) || value.len() + key.len() + 1 > MAX_LINE_BYTES {
                return Err(ProtocolError::InvalidLine);
            }
            output.push_str(&format!("{key} {value}\n"));
        }
    }
    for (key, value) in [
        ("foxMoveNumber", snapshot.fox_move_number),
        ("liveTitleMove", snapshot.context.live_title_move),
        ("recordCurrentMove", snapshot.context.record_current_move),
        ("recordTotalMove", snapshot.context.record_total_move),
    ] {
        if let Some(value) = value {
            output.push_str(&format!("{key} {value}\n"));
        }
    }
    if let Some(value) = snapshot.context.record_at_end {
        output.push_str(if value {
            "recordAtEnd 1\n"
        } else {
            "recordAtEnd 0\n"
        });
    }
    if snapshot.force_rebuild {
        output.push_str("forceRebuild\n");
    }
    for row in snapshot.cells.chunks(usize::from(snapshot.size.width)) {
        output.push_str("re=");
        for (index, cell) in row.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            output.push(char::from(b'0' + cell.code()));
        }
        output.push('\n');
    }
    output.push_str("end\n");
    Ok(output)
}

#[cfg(test)]
mod tests;
