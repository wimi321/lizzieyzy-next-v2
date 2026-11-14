use crate::{MAX_RESPONSE_BYTES, ProtocolError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub id: Option<u32>,
    pub success: bool,
    pub body: String,
}

impl Response {
    pub fn encode(&self) -> Result<String, ProtocolError> {
        if self.body.split('\n').any(|line| line.trim().is_empty()) && !self.body.is_empty()
            || self
                .body
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(ProtocolError::InvalidResponse(
                "body contains an empty line or control character".into(),
            ));
        }
        let mut output = if self.success { "=" } else { "?" }.to_owned();
        if let Some(id) = self.id {
            output.push_str(&id.to_string());
        }
        if !self.body.is_empty() {
            output.push(' ');
            output.push_str(&self.body);
        }
        output.push_str("\n\n");
        if output.len() > MAX_RESPONSE_BYTES {
            return Err(ProtocolError::TooLarge);
        }
        Ok(output)
    }
}

#[derive(Default)]
pub struct Decoder {
    response: Option<Response>,
    bytes: usize,
}

impl Decoder {
    /// Accepts one line without its LF or CRLF. Empty lines delimit replies.
    pub fn push_line(&mut self, line: &str) -> Result<Option<Response>, ProtocolError> {
        let result = self.push(line);
        if result.is_err() {
            self.response = None;
            self.bytes = 0;
        }
        result
    }

    fn push(&mut self, line: &str) -> Result<Option<Response>, ProtocolError> {
        self.bytes = self.bytes.saturating_add(line.len().saturating_add(1));
        if self.bytes > MAX_RESPONSE_BYTES {
            return Err(ProtocolError::TooLarge);
        }
        if line.chars().any(|c| c.is_control() && c != '\t') {
            return Err(ProtocolError::InvalidResponse(
                "embedded control character".into(),
            ));
        }
        if line.trim().is_empty() {
            self.bytes = 0;
            return Ok(self.response.take());
        }
        if let Some(response) = self.response.as_mut() {
            if !response.body.is_empty() {
                response.body.push('\n');
            }
            response.body.push_str(line);
            return Ok(None);
        }
        let marker = line.as_bytes()[0];
        if marker != b'=' && marker != b'?' {
            return Err(ProtocolError::InvalidResponse(
                "missing = or ? marker".into(),
            ));
        }
        let rest = &line[1..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let id = if digits == 0 {
            None
        } else {
            Some(
                rest[..digits]
                    .parse()
                    .map_err(|_| ProtocolError::InvalidResponse("id out of range".into()))?,
            )
        };
        let body = &rest[digits..];
        if !body.is_empty() && !body.starts_with([' ', '\t']) {
            return Err(ProtocolError::InvalidResponse(
                "missing whitespace after marker/id".into(),
            ));
        }
        self.response = Some(Response {
            id,
            success: marker == b'=',
            body: body.trim_start_matches([' ', '\t']).into(),
        });
        Ok(None)
    }

    pub fn finish(self) -> Result<(), ProtocolError> {
        if self.response.is_some() {
            Err(ProtocolError::IncompleteResponse)
        } else {
            Ok(())
        }
    }
}

pub fn parse_response(text: &str) -> Result<Response, ProtocolError> {
    let mut decoder = Decoder::default();
    let mut result = None;
    for line in text.lines() {
        if let Some(response) = decoder.push_line(line)?
            && result.replace(response).is_some()
        {
            return Err(ProtocolError::InvalidResponse("multiple responses".into()));
        }
    }
    decoder.finish()?;
    result.ok_or(ProtocolError::IncompleteResponse)
}
