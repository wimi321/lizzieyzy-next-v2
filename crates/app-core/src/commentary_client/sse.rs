use super::{CommentaryClientError, map_network};
use reqwest::Response;
use serde_json::Value;

const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_ANSWER_CHARS: usize = 100_000;

#[derive(Default)]
struct Decoder {
    line: Vec<u8>,
    data: String,
    event: String,
    after_cr: bool,
}

impl Decoder {
    fn push(&mut self, byte: u8) -> Result<Option<(String, String)>, CommentaryClientError> {
        if byte == b'\n' && self.after_cr {
            self.after_cr = false;
            return Ok(None);
        }
        self.after_cr = byte == b'\r';
        if byte != b'\n' && byte != b'\r' {
            self.line.push(byte);
            if self.line.len() + self.data.len() > MAX_FRAME_BYTES {
                return Err(CommentaryClientError::InvalidResponse);
            }
            return Ok(None);
        }
        let line = String::from_utf8(std::mem::take(&mut self.line))
            .map_err(|_| CommentaryClientError::InvalidResponse)?;
        if line.is_empty() {
            let event = std::mem::take(&mut self.event);
            let data = std::mem::take(&mut self.data);
            return Ok((!data.is_empty()).then_some((event, data)));
        }
        if line.starts_with(':') {
            return Ok(None);
        }
        let (field, value) = line.split_once(':').unwrap_or((&line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "data" => {
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(value);
            }
            "event" => self.event = value.to_owned(),
            _ => {}
        }
        Ok(None)
    }
}

pub(super) async fn read(
    mut response: Response,
    responses: bool,
    on_delta: &mut impl FnMut(String),
) -> Result<String, CommentaryClientError> {
    let mut decoder = Decoder::default();
    let mut answer = String::new();
    let mut answer_chars = 0;
    while let Some(bytes) = response.chunk().await.map_err(|error| {
        if error.is_timeout() {
            map_network(error)
        } else {
            CommentaryClientError::Interrupted
        }
    })? {
        for &byte in &bytes {
            let Some((event, data)) = decoder.push(byte)? else {
                continue;
            };
            if data.trim() == "[DONE]" {
                return if responses {
                    Err(CommentaryClientError::Interrupted)
                } else {
                    Ok(answer)
                };
            }
            let value: Value =
                serde_json::from_str(&data).map_err(|_| CommentaryClientError::InvalidResponse)?;
            let (delta, completed, interrupted) = if responses {
                let kind = value.get("type").and_then(Value::as_str).unwrap_or(&event);
                match kind {
                    "response.output_text.delta" => {
                        (value.get("delta").and_then(Value::as_str), false, false)
                    }
                    "response.completed" => (None, true, false),
                    "response.failed" | "response.incomplete" | "error" => {
                        return Err(CommentaryClientError::Interrupted);
                    }
                    _ => (None, false, false),
                }
            } else {
                if value.get("error").is_some() {
                    return Err(CommentaryClientError::Interrupted);
                }
                let choice = value
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|items| items.first());
                let finish = choice
                    .and_then(|choice| choice.get("finish_reason"))
                    .and_then(Value::as_str);
                (
                    choice
                        .and_then(|choice| choice.get("delta"))
                        .and_then(|delta| delta.get("content"))
                        .and_then(Value::as_str),
                    finish == Some("stop"),
                    finish.is_some_and(|reason| reason != "stop"),
                )
            };
            if let Some(delta) = delta.filter(|delta| !delta.is_empty()) {
                answer_chars += delta.chars().count();
                if answer_chars > MAX_ANSWER_CHARS {
                    return Err(CommentaryClientError::InvalidResponse);
                }
                answer.push_str(delta);
                on_delta(delta.to_owned());
            }
            if interrupted {
                return Err(CommentaryClientError::Interrupted);
            }
            if completed {
                return Ok(answer);
            }
        }
    }
    // In particular, an EOF without the blank line terminating a completion
    // event does not turn a truncated stream into a successful answer.
    Err(CommentaryClientError::Interrupted)
}
