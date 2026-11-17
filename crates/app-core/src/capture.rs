//! In-memory screen-capture inputs and actor-backed life-and-death recognition.
//! The shell supplies pixels; this module never accesses a display or a file.

use crate::{AppCore, AppError, actor};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::oneshot;
use ts_rs::TS;

mod conversion;

/// Enough for an 8K RGBA monitor; bounds allocations from untrusted IPC inputs.
pub const MAX_CAPTURE_BYTES: usize = 128 * 1024 * 1024;

/// Raw pixels are deliberately absent from JSON, TS and Debug output.
#[derive(Clone)]
pub struct CaptureImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub scale_factor: f64,
}

impl CaptureImage {
    pub fn validate(&self) -> Result<(), CaptureError> {
        let bytes = u64::from(self.width)
            .checked_mul(u64::from(self.height))
            .and_then(|pixels| pixels.checked_mul(4));
        if self.width == 0
            || self.height == 0
            || bytes != Some(self.rgba.len() as u64)
            || self.rgba.len() > MAX_CAPTURE_BYTES
        {
            return Err(CaptureError::InvalidImage);
        }
        if !self.scale_factor.is_finite() || self.scale_factor <= 0.0 {
            return Err(CaptureError::InvalidScaleFactor);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct CaptureSettings {
    #[ts(as = "Option<u16>", optional)]
    pub black_offset: u16,
    #[ts(as = "Option<u16>", optional)]
    pub black_percent: u16,
    #[ts(as = "Option<u16>", optional)]
    pub white_offset: u16,
    #[ts(as = "Option<u16>", optional)]
    pub white_percent: u16,
    #[ts(as = "Option<u16>", optional)]
    pub gray_offset: u16,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            black_offset: 120,
            black_percent: 25,
            white_offset: 120,
            white_percent: 15,
            gray_offset: 80,
        }
    }
}

impl CaptureSettings {
    pub fn validate(&self) -> Result<(), CaptureError> {
        for (field, value, minimum, maximum) in [
            ("black_offset", self.black_offset, 0, 255),
            ("white_offset", self.white_offset, 0, 255),
            ("gray_offset", self.gray_offset, 0, 255),
            ("black_percent", self.black_percent, 1, 100),
            ("white_percent", self.white_percent, 1, 100),
        ] {
            if !(minimum..=maximum).contains(&value) {
                return Err(CaptureError::InvalidSettings {
                    field: field.into(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CaptureSession {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CapturePoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CaptureCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CaptureCell {
    Empty,
    Black,
    White,
}

/// The cropped grid in screen order, row-major from the top-left intersection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CaptureCells {
    pub rows: u32,
    pub columns: u32,
    pub cells: Vec<CaptureCell>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CaptureGrid {
    pub origin: CapturePoint,
    pub gap_x: f64,
    pub gap_y: f64,
    pub columns: u32,
    pub rows: u32,
    pub corner: CaptureCorner,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CaptureRecognition {
    pub grid: CaptureGrid,
    pub cells: CaptureCells,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CaptureResult {
    pub corner: CaptureCorner,
    pub cells: CaptureCells,
    pub board_size: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum CaptureOutcome {
    Finished,
    Cancelled,
    Failed { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum CaptureError {
    #[error("there is no capture session")]
    NoSession,
    #[error("the capture session has expired")]
    StaleSession,
    #[error("invalid capture pixels or image dimensions")]
    InvalidImage,
    #[error("capture scale factor must be finite and positive")]
    InvalidScaleFactor,
    #[error("capture session identifiers are exhausted")]
    SessionLimit,
    #[error("invalid capture threshold: {field}")]
    InvalidSettings { field: String },
    #[error("cannot fit the capture grid: {reason}")]
    GridFit { reason: String },
    #[error("capture does not fit on the selected board")]
    DoesNotFit,
    #[error("capture cell dimensions do not match its contents")]
    InvalidCells,
    #[error("capture board size must be in 2..=25")]
    InvalidBoardSize,
    #[error("the engine default rules cannot be represented by a new game")]
    UnsupportedRules,
    #[error("capture recognition failed: {reason}")]
    RecognitionFailed { reason: String },
}

pub(crate) type Reply<T> = oneshot::Sender<Result<T, AppError>>;
pub(crate) enum Request {
    Begin(CaptureImage, Reply<CaptureSession>),
    Image(u32, Reply<Arc<CaptureImage>>),
    Prepare(u32, Reply<(Arc<CaptureImage>, CaptureSettings)>),
    Validate(u32, Reply<()>),
    Cancel(u32, Reply<()>),
    Fail(u32, String, Reply<()>),
    ReportFailure(String, Reply<()>),
}

impl AppCore {
    pub async fn capture_settings(&self) -> Result<CaptureSettings, AppError> {
        Ok(self.settings().await?.capture)
    }

    pub async fn set_capture_settings(
        &self,
        settings: CaptureSettings,
    ) -> Result<CaptureSettings, AppError> {
        self.send(actor::Input::SetCaptureSettings(settings))
            .await?;
        Ok(settings)
    }

    /// Replace the previous in-memory session only after validating the image.
    pub async fn begin_capture(&self, image: CaptureImage) -> Result<CaptureSession, AppError> {
        self.capture_request(|reply| Request::Begin(image, reply))
            .await
    }

    /// A shared immutable image; callers must release their copy after encoding.
    pub async fn capture_image(&self, id: u32) -> Result<Arc<CaptureImage>, AppError> {
        self.capture_request(|reply| Request::Image(id, reply))
            .await
    }

    pub async fn recognize_capture(
        &self,
        id: u32,
        points: [CapturePoint; 3],
    ) -> Result<CaptureRecognition, AppError> {
        self.recognize_with(id, move |image, settings| {
            conversion::recognize(&image, points, settings)
        })
        .await
    }

    async fn recognize_with(
        &self,
        id: u32,
        recognize: impl FnOnce(
            Arc<CaptureImage>,
            CaptureSettings,
        ) -> Result<CaptureRecognition, CaptureError>
        + Send
        + 'static,
    ) -> Result<CaptureRecognition, AppError> {
        let (image, settings) = self
            .capture_request(|reply| Request::Prepare(id, reply))
            .await?;
        // Pixels are read outside the app actor. Cancellation/replacement can be
        // processed while CPU work runs; the second check rejects its stale reply.
        let result = tokio::task::spawn_blocking(move || recognize(image, settings))
            .await
            .map_err(|error| CaptureError::RecognitionFailed {
                reason: error.to_string(),
            });
        self.capture_request(|reply| Request::Validate(id, reply))
            .await?;
        Ok(result??)
    }

    pub async fn finish_capture(&self, id: u32, result: CaptureResult) -> Result<u64, AppError> {
        self.send(actor::Input::FinishCapture(id, result)).await
    }

    pub async fn cancel_capture(&self, id: u32) -> Result<(), AppError> {
        self.capture_request(|reply| Request::Cancel(id, reply))
            .await
    }

    /// Shell cleanup after an error; stale IDs cannot end a replacement session.
    pub async fn fail_capture(&self, id: u32, reason: String) -> Result<(), AppError> {
        self.capture_request(|reply| Request::Fail(id, reason, reply))
            .await
    }

    /// Shell errors before it has supplied an image still reach the main window.
    pub async fn report_capture_failure(&self, reason: String) -> Result<(), AppError> {
        self.capture_request(|reply| Request::ReportFailure(reason, reply))
            .await
    }

    async fn capture_request<T>(
        &self,
        build: impl FnOnce(Reply<T>) -> Request,
    ) -> Result<T, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::Capture(build(reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

#[cfg(test)]
mod tests;
