//! Binary PNG IPC framing. PNG validation and file operations stay in AppCore.
use tauri::ipc::{InvokeBody, Request};

use crate::error::{CommandError, PngRequestError};

pub(crate) const PATH_HEADER: &str = "x-lizzieyzy-path";

pub(crate) fn bytes<'a>(request: &'a Request<'_>) -> Result<&'a [u8], CommandError> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(PngRequestError::RawBodyRequired.into());
    };
    // Reject before cloning a potentially large transport buffer. AppCore
    // independently enforces the same bound for non-IPC callers.
    if bytes.len() > app_core::MAX_PNG_BYTES {
        return Err(CommandError::Png(app_core::PngError::TooLarge));
    }
    Ok(bytes)
}

pub(crate) fn path(request: &Request<'_>) -> Result<String, PngRequestError> {
    let mut headers = request.headers().get_all(PATH_HEADER).iter();
    let encoded = headers.next().ok_or(PngRequestError::MissingPath)?;
    if headers.next().is_some() {
        return Err(PngRequestError::InvalidPathEncoding);
    }
    let encoded = encoded
        .to_str()
        .map_err(|_| PngRequestError::InvalidPathEncoding)?;
    let mut input = encoded.bytes();
    let mut decoded = Vec::with_capacity(encoded.len());
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = input.next().and_then(hex);
            let low = input.next().and_then(hex);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(PngRequestError::InvalidPathEncoding);
            };
            decoded.push(high * 16 + low);
        } else {
            // encodeURIComponent does not use application/x-www-form-urlencoded
            // semantics: an unescaped '+' is a literal plus, never a space.
            decoded.push(byte);
        }
    }
    let path = String::from_utf8(decoded).map_err(|_| PngRequestError::InvalidPathEncoding)?;
    if path.is_empty() {
        return Err(PngRequestError::EmptyPath);
    }
    Ok(path)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
