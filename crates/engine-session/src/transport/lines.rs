//! Bounded local-process lines. All consumed bytes and discard state live in
//! the reader, so cancelling `next_line` in a select never loses a partial line.

use std::io;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

pub(super) const STDOUT_LIMIT: usize = 8 * 1024 * 1024;
pub(super) const STDERR_LIMIT: usize = 4096;

pub(super) struct ByteLines<R> {
    reader: BufReader<R>,
    prefix: Vec<u8>,
    limit: usize,
    // Saturates at limit + 2: one extra CR is allowed before LF, two extra
    // bytes always mean overflow. No counter grows with discarded output.
    length: usize,
    last: Option<u8>,
}

pub(super) struct Line {
    bytes: Vec<u8>,
    truncated: bool,
}

impl<R: AsyncRead + Unpin> ByteLines<R> {
    pub(super) fn new(reader: R, limit: usize) -> Self {
        Self {
            reader: BufReader::new(reader),
            prefix: Vec::new(),
            limit,
            length: 0,
            last: None,
        }
    }

    pub(super) async fn next_line(&mut self) -> io::Result<Option<Line>> {
        loop {
            let available = self.reader.fill_buf().await?;
            if available.is_empty() {
                return Ok((self.length != 0).then(|| self.finish(false)));
            }
            let newline = available.iter().position(|&byte| byte == b'\n');
            let count = newline.unwrap_or(available.len());
            let keep = count.min(self.limit - self.prefix.len());
            let needed = self.prefix.len() + keep;
            if needed > self.prefix.capacity() {
                // Vec's default doubling can overshoot the limit for uneven
                // chunks. Grow geometrically, but cap the allocation as well.
                let capacity = needed.max(self.prefix.capacity() * 2).min(self.limit);
                self.prefix.reserve_exact(capacity - self.prefix.len());
            }
            self.prefix.extend_from_slice(&available[..keep]);
            self.length = (self.length + count).min(self.limit + 2);
            if count != 0 {
                self.last = Some(available[count - 1]);
            }
            // No await between recording bytes and consuming them.
            self.reader.consume(count + usize::from(newline.is_some()));
            if newline.is_some() {
                return Ok(Some(self.finish(true)));
            }
        }
    }

    fn finish(&mut self, newline: bool) -> Line {
        if newline && self.last == Some(b'\r') {
            if self.length <= self.limit {
                self.prefix.pop();
            }
            self.length -= 1;
        }
        let line = Line {
            bytes: std::mem::take(&mut self.prefix),
            truncated: self.length > self.limit,
        };
        self.length = 0;
        self.last = None;
        line
    }
}

impl Line {
    pub(super) fn stdout(self) -> super::Output {
        if self.truncated {
            return super::Output::MalformedStdout("stdout line exceeds 8 MiB");
        }
        match String::from_utf8(self.bytes) {
            Ok(line) => super::Output::Stdout(line),
            Err(_) => super::Output::MalformedStdout("stdout line is not UTF-8"),
        }
    }

    pub(super) fn stderr(self) -> String {
        let bytes = &self.bytes;
        let decoded = match std::str::from_utf8(bytes) {
            Ok(line) => line.to_owned(),
            // A valid UTF-8 log can be cut in the middle of its last character.
            // Keep the complete prefix rather than misidentifying it as ANSI.
            Err(error) if self.truncated && error.error_len().is_none() => {
                String::from_utf8_lossy(&bytes[..error.valid_up_to()]).into_owned()
            }
            Err(_) => decode_stderr(bytes),
        };
        // ANSI and lossy decoding can expand bytes. Keep the existing bound on
        // the UTF-8 diagnostic tail and broadcasts as well as the raw prefix.
        crate::startup::bounded_line(decoded)
    }
}

fn decode_stderr(bytes: &[u8]) -> String {
    #[cfg(windows)]
    if let Some(decoded) = ansi(bytes) {
        return decoded;
    }
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn ansi(bytes: &[u8]) -> Option<String> {
    use windows_sys::Win32::Globalization::{CP_ACP, MB_ERR_INVALID_CHARS, MultiByteToWideChar};

    let length = i32::try_from(bytes.len()).ok()?;
    // One UTF-16 unit per input byte is sufficient for ANSI (including UTF-8
    // ACP). The local reader bounds the input to 4096 bytes.
    let mut wide = vec![0u16; bytes.len()];
    // SAFETY: both buffers are live, nonoverlapping and length-checked. The API
    // writes at most `length` UTF-16 units and retains neither pointer.
    let written = unsafe {
        MultiByteToWideChar(
            CP_ACP,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            length,
            wide.as_mut_ptr(),
            length,
        )
    };
    if written <= 0 {
        return None;
    }
    String::from_utf16(&wide[..written as usize]).ok()
}

#[cfg(test)]
mod tests;
