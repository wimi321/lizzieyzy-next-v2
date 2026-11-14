//! Local link import only: no connections, credentials, or system keychain access.
use std::{io::Read, path::PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;

/// Errors deliberately contain neither the imported link nor the image path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, TS)]
pub enum RemoteLinkError {
    #[error("a ws:// or wss:// link with a host is required")]
    InvalidLink,
    #[error("cannot read the QR image")]
    ImageRead,
    #[error("a valid PNG or JPEG image is required")]
    InvalidImage,
    #[error("the QR image exceeds the supported size")]
    ImageTooLarge,
    #[error("no QR code was found in the image")]
    NoQrCode,
    #[error("the QR code could not be decoded")]
    QrDecode,
}

/// Match Java's ASCII-width conversion and pasted-link extraction, without its
/// special replacement for the unrelated `完善://` scheme. Keep the original
/// spelling of the extracted URL so signed paths and tokens are not rewritten.
pub fn normalize_remote_link(text: &str) -> Result<String, RemoteLinkError> {
    let normalized: String = text
        .trim()
        .chars()
        .map(|c| match c {
            '\u{3000}' => ' ',
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            _ => c,
        })
        .collect();
    let mut text = normalized.trim();
    if text.len() >= 2
        && ((text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\'')))
    {
        text = text[1..text.len() - 1].trim();
    }
    let lower = text.to_ascii_lowercase();
    let start = [lower.find("ws://"), lower.find("wss://")]
        .into_iter()
        .flatten()
        .min()
        .ok_or(RemoteLinkError::InvalidLink)?;
    let text = &text[start..];
    let end = text
        .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>'))
        .unwrap_or(text.len());
    let link = &text[..end];
    // URL parsers can repair an absent authority or turn backslashes into '/'.
    // A pasted engine link must already have an explicit, unambiguous host.
    let authority = link
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default());
    if authority.is_none_or(str::is_empty)
        || link.contains('\\')
        || link.chars().any(char::is_control)
    {
        return Err(RemoteLinkError::InvalidLink);
    }
    let parsed = url::Url::parse(link).map_err(|_| RemoteLinkError::InvalidLink)?;
    if !matches!(parsed.scheme(), "ws" | "wss") || parsed.host_str().is_none_or(str::is_empty) {
        return Err(RemoteLinkError::InvalidLink);
    }
    Ok(link.to_owned())
}

/// File I/O, image decoding, and QR recognition stay off the application actor.
pub async fn read_remote_link_qr(path: PathBuf) -> Result<String, RemoteLinkError> {
    tokio::task::spawn_blocking(move || read_qr_image(path))
        .await
        .map_err(|_| RemoteLinkError::QrDecode)?
}

fn read_qr_image(path: PathBuf) -> Result<String, RemoteLinkError> {
    let metadata = std::fs::metadata(&path).map_err(|_| RemoteLinkError::ImageRead)?;
    if !metadata.is_file() {
        return Err(RemoteLinkError::InvalidImage);
    }
    if metadata.len() > MAX_IMAGE_BYTES {
        return Err(RemoteLinkError::ImageTooLarge);
    }
    let file = std::fs::File::open(path).map_err(|_| RemoteLinkError::ImageRead)?;
    let metadata = file.metadata().map_err(|_| RemoteLinkError::ImageRead)?;
    if !metadata.is_file() {
        return Err(RemoteLinkError::InvalidImage);
    }
    if metadata.len() > MAX_IMAGE_BYTES {
        return Err(RemoteLinkError::ImageTooLarge);
    }
    let mut bytes = Vec::new();
    file.take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| RemoteLinkError::ImageRead)?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(RemoteLinkError::ImageTooLarge);
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| RemoteLinkError::InvalidImage)?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
    ) {
        return Err(RemoteLinkError::InvalidImage);
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|error| match error {
        image::ImageError::Limits(_) => RemoteLinkError::ImageTooLarge,
        _ => RemoteLinkError::InvalidImage,
    })?;
    if u64::from(decoded.width()) * u64::from(decoded.height()) > MAX_IMAGE_PIXELS {
        return Err(RemoteLinkError::ImageTooLarge);
    }
    let mut prepared = rqrr::PreparedImage::prepare(decoded.to_luma8());
    let grids = prepared.detect_grids();
    if grids.is_empty() {
        return Err(RemoteLinkError::NoQrCode);
    }
    let mut failure = RemoteLinkError::QrDecode;
    for grid in grids {
        if let Ok((_, text)) = grid.decode() {
            match normalize_remote_link(&text) {
                Ok(link) => return Ok(link),
                Err(error) => failure = error,
            }
        }
    }
    Err(failure)
}

#[cfg(test)]
mod tests;
