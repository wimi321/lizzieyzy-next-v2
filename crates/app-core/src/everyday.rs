//! Native clipboard boundary and everyday export commands. No WebView or OS I/O
//! is embedded here; the host injects a clipboard and tests inject a fake.
use crate::{AppCore, AppError, actor};
use serde::{Deserialize, Serialize};
use std::{future::Future, path::PathBuf, pin::Pin};
use ts_rs::TS;

pub const MAX_PNG_BYTES: usize = 32 * 1024 * 1024;
const MAX_RGBA_BYTES: usize = 128 * 1024 * 1024;

pub type ClipboardFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ClipboardError>> + Send + 'a>>;

pub trait Clipboard: Send + Sync + std::fmt::Debug {
    fn read_text(&self) -> ClipboardFuture<'_, Option<String>>;
    fn write_text(&self, text: String) -> ClipboardFuture<'_, ()>;
    fn write_image(&self, image: ClipboardImage) -> ClipboardFuture<'_, ()>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "type", content = "data")]
pub enum ClipboardError {
    #[error("native clipboard is unavailable")]
    Unavailable,
    #[error("clipboard contains no text")]
    EmptyText,
    #[error("native clipboard failed: {0}")]
    Platform(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "type", content = "data")]
pub enum PngError {
    #[error("PNG exceeds the 32 MiB input limit")]
    TooLarge,
    #[error("invalid PNG: {0}")]
    Invalid(String),
    #[error("PNG exceeds the 128 MiB decoded pixel limit")]
    DimensionsTooLarge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum BoardTransform {
    RotateRight,
    RotateLeft,
    FlipHorizontal,
    FlipVertical,
}

impl From<BoardTransform> for document::BoardTransform {
    fn from(value: BoardTransform) -> Self {
        match value {
            BoardTransform::RotateRight => Self::RotateRight,
            BoardTransform::RotateLeft => Self::RotateLeft,
            BoardTransform::FlipHorizontal => Self::FlipHorizontal,
            BoardTransform::FlipVertical => Self::FlipVertical,
        }
    }
}

impl AppCore {
    pub async fn copy_sgf(&self) -> Result<(), AppError> {
        self.send(actor::Input::CopySgf).await.map(|_| ())
    }

    pub async fn paste_sgf(&self) -> Result<u64, AppError> {
        self.send(actor::Input::PasteSgf).await
    }

    pub async fn save_clean_sgf(
        &self,
        path: impl Into<PathBuf>,
        keep_comments: bool,
        current_branch: bool,
    ) -> Result<(), AppError> {
        self.send(actor::Input::SaveCleanSgf(
            path.into(),
            keep_comments,
            current_branch,
        ))
        .await
        .map(|_| ())
    }

    pub async fn save_png(&self, path: impl Into<PathBuf>, bytes: Vec<u8>) -> Result<(), AppError> {
        self.send(actor::Input::SavePng(path.into(), bytes))
            .await
            .map(|_| ())
    }

    pub async fn copy_png(&self, bytes: Vec<u8>) -> Result<(), AppError> {
        self.send(actor::Input::CopyPng(bytes)).await.map(|_| ())
    }

    pub async fn transform_board(&self, transform: BoardTransform) -> Result<u64, AppError> {
        self.apply(document::Command::Transform(transform.into()))
            .await
    }
}

pub(crate) async fn decode_png(bytes: Vec<u8>) -> Result<(Vec<u8>, ClipboardImage), AppError> {
    if bytes.len() > MAX_PNG_BYTES {
        return Err(PngError::TooLarge.into());
    }
    tokio::task::spawn_blocking(move || {
        let image = decode(&bytes)?;
        Ok((bytes, image))
    })
    .await
    .map_err(std::io::Error::other)?
}

fn decode(bytes: &[u8]) -> Result<ClipboardImage, AppError> {
    let invalid = |error: png::DecodingError| PngError::Invalid(error.to_string());
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: MAX_RGBA_BYTES,
    });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(invalid)?;
    let info = reader.info();
    let pixels = u64::from(info.width) * u64::from(info.height);
    if pixels == 0 || pixels > (MAX_RGBA_BYTES / 4) as u64 {
        return Err(PngError::DimensionsTooLarge.into());
    }
    if info.animation_control.is_some() {
        return Err(PngError::Invalid("animated PNG is not a static export".into()).into());
    }
    let length = reader
        .output_buffer_size()
        .filter(|length| *length <= MAX_RGBA_BYTES)
        .ok_or(PngError::DimensionsTooLarge)?;
    let mut buffer = vec![0; length];
    let output = reader.next_frame(&mut buffer).map_err(invalid)?;
    reader.finish().map_err(invalid)?;
    let mut rgba = Vec::with_capacity(pixels as usize * 4);
    for pixel in buffer[..output.buffer_size()].chunks_exact(output.color_type.samples()) {
        match output.color_type {
            png::ColorType::Rgba => rgba.extend_from_slice(pixel),
            png::ColorType::Rgb => rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]),
            png::ColorType::Grayscale => {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], 255])
            }
            png::ColorType::GrayscaleAlpha => {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]])
            }
            png::ColorType::Indexed => {
                return Err(PngError::Invalid("unexpanded palette".into()).into());
            }
        }
    }
    Ok(ClipboardImage {
        rgba,
        width: output.width,
        height: output.height,
    })
}

#[cfg(test)]
mod tests;
