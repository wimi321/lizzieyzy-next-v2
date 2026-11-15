use crate::RemoteTransportError as Error;
use serde_json::Value;
use std::time::Duration;
use tokio::time::Instant;

pub(super) enum Packet {
    Open,
    Connected,
    Event { name: String, payload: Vec<u8> },
    Failure(Error),
}

pub(super) struct Parser {
    opened: bool,
    heartbeat: Duration,
    deadline: Instant,
    binary_event: Option<String>,
}
impl Default for Parser {
    fn default() -> Self {
        Self {
            opened: false,
            heartbeat: Duration::from_secs(30),
            deadline: Instant::now() + Duration::from_secs(30),
            binary_event: None,
        }
    }
}

impl Parser {
    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(super) fn heartbeat(&mut self) {
        self.deadline = Instant::now() + self.heartbeat;
    }

    pub(super) fn text(&mut self, text: &str) -> Result<Option<Packet>, Error> {
        if let Some(open) = text.strip_prefix('0') {
            if self.opened {
                return Err(Error::Protocol);
            }
            let open: Value = serde_json::from_str(open).map_err(|_| Error::Protocol)?;
            let interval = open
                .get("pingInterval")
                .and_then(Value::as_u64)
                .ok_or(Error::Protocol)?;
            let timeout = open
                .get("pingTimeout")
                .and_then(Value::as_u64)
                .ok_or(Error::Protocol)?;
            let millis = interval
                .checked_add(timeout)
                .filter(|value| (1..=600_000).contains(value))
                .ok_or(Error::Protocol)?;
            self.heartbeat = Duration::from_millis(millis);
            self.heartbeat();
            self.opened = true;
            return Ok(Some(Packet::Open));
        }
        if !self.opened {
            return Err(Error::Protocol);
        }
        if text == "40" || text.starts_with("40{") {
            return Ok(Some(Packet::Connected));
        }
        if text == "41" || text == "1" {
            return Err(Error::Connect);
        }
        if let Some(error) = text.strip_prefix("44") {
            let value: Value = serde_json::from_str(error).map_err(|_| Error::Protocol)?;
            return Ok(Some(Packet::Failure(failure(&value))));
        }
        if let Some(event) = text.strip_prefix("42") {
            let (name, value) = event_fields(event)?;
            if matches!(name.as_str(), "error" | "connect_error") {
                return Ok(Some(Packet::Failure(failure(&value))));
            }
            let payload = match value {
                Value::String(text) => text.into_bytes(),
                Value::Null if name == "ready" => Vec::new(),
                _ if !matches!(name.as_str(), "stdout" | "stderr") => Vec::new(),
                _ => return Err(Error::Protocol),
            };
            return Ok(Some(Packet::Event { name, payload }));
        }
        if let Some(event) = text.strip_prefix("451-") {
            if self.binary_event.is_some() {
                return Err(Error::Protocol);
            }
            let (name, payload) = event_fields(event)?;
            if payload.get("_placeholder").and_then(Value::as_bool) != Some(true)
                || payload.get("num").and_then(Value::as_u64) != Some(0)
            {
                return Err(Error::Protocol);
            }
            self.binary_event = Some(name);
            return Ok(None);
        }
        // Only the default namespace and single-payload binary events are used.
        Err(Error::Protocol)
    }

    pub(super) fn binary(&mut self, bytes: &[u8]) -> Result<Packet, Error> {
        let name = self.binary_event.take().ok_or(Error::Protocol)?;
        Ok(Packet::Event {
            name,
            payload: bytes.to_vec(),
        })
    }
}

fn event_fields(event: &str) -> Result<(String, Value), Error> {
    let mut values: Vec<Value> = serde_json::from_str(event).map_err(|_| Error::Protocol)?;
    if values.is_empty() || values.len() > 2 {
        return Err(Error::Protocol);
    }
    let payload = if values.len() == 2 {
        values.pop().ok_or(Error::Protocol)?
    } else {
        Value::Null
    };
    let name = values
        .pop()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or(Error::Protocol)?;
    Ok((name, payload))
}

fn failure(value: &Value) -> Error {
    let source = value
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(value);
    let status = source
        .get("status")
        .or_else(|| source.get("statusCode"))
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok());
    Error::zhizi_fatal(
        status,
        source
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
    .unwrap_or(Error::Connect)
}
