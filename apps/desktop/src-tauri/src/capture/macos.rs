//! macOS 14+ screenshots via ScreenCaptureKit's SCScreenshotManager.
//! Native calls in this module are compiled, never invoked by automated tests.

use app_core::{CaptureImage, MAX_CAPTURE_BYTES};
use screencapturekit::{
    error::SCError,
    screenshot_manager::{CGImageExt, SCScreenshotManager},
    shareable_content::SCShareableContent,
    stream::{
        configuration::{PixelFormat, SCStreamConfiguration},
        content_filter::SCContentFilter,
    },
};

use super::{Display, ScreenCaptureError, failure};

#[cfg(test)]
mod tests;

pub(super) fn capture(display: Display) -> Result<CaptureImage, ScreenCaptureError> {
    let content = SCShareableContent::get().map_err(capture_error)?;
    let screen = content
        .displays()
        .into_iter()
        .find(|screen| screen.display_id() == display.id)
        .ok_or(ScreenCaptureError::DisplayUnavailable)?;
    let filter = SCContentFilter::create()
        .with_display(&screen)
        .with_excluding_windows(&[])
        .build()
        .map_err(capture_error)?;

    // The safe style accessor reports FeatureNotAvailable on macOS < 14;
    // unlike content_rect(), it does not silently return an empty default.
    filter.style().map_err(capture_error)?;

    // The filter reports points; its own scale preserves Retina pixels.
    let rect = filter.content_rect();
    let (width, height) = pixel_dimensions(
        rect.size.width,
        rect.size.height,
        f64::from(filter.point_pixel_scale()),
    )?;
    let config = SCStreamConfiguration::new()
        .with_width(width)
        .with_height(height)
        .with_pixel_format(PixelFormat::BGRA)
        .with_shows_cursor(false);
    let image = SCScreenshotManager::capture_image(&filter, &config).map_err(capture_error)?;
    image_rgba(&image)
}

pub(super) fn image_rgba(
    image: &screencapturekit::CGImage,
) -> Result<CaptureImage, ScreenCaptureError> {
    let width = u32::try_from(image.width()).map_err(failure)?;
    let height = u32::try_from(image.height()).map_err(failure)?;
    check_image_size(width, height)?;
    // This safe helper renders the returned CGImage into tightly packed RGBA;
    // it does not expose its native BGRA/color-space representation to app-core.
    let image = CaptureImage {
        width,
        height,
        rgba: image.rgba_data().map_err(capture_error)?,
        // The outer shell derives the final ratio from this image and monitor bounds.
        scale_factor: 1.0,
    };
    image.validate().map_err(failure)?;
    Ok(image)
}

pub(super) fn pixel_dimensions(
    width_points: f64,
    height_points: f64,
    scale: f64,
) -> Result<(u32, u32), ScreenCaptureError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(failure(
            "ScreenCaptureKit returned an invalid display scale",
        ));
    }
    let width = (width_points * scale).round();
    let height = (height_points * scale).round();
    if [width, height]
        .iter()
        .any(|value| !value.is_finite() || *value < 1.0 || *value > f64::from(u32::MAX))
    {
        return Err(failure(
            "ScreenCaptureKit returned invalid image dimensions",
        ));
    }
    let dimensions = (width as u32, height as u32);
    check_image_size(dimensions.0, dimensions.1)?;
    Ok(dimensions)
}

fn check_image_size(width: u32, height: u32) -> Result<(), ScreenCaptureError> {
    let bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4));
    if width == 0 || height == 0 || bytes.is_none_or(|bytes| bytes > MAX_CAPTURE_BYTES as u64) {
        return Err(failure(
            "ScreenCaptureKit image exceeds the capture size limit",
        ));
    }
    Ok(())
}

pub(super) fn capture_error(error: SCError) -> ScreenCaptureError {
    if matches!(error, SCError::FeatureNotAvailable { .. }) {
        ScreenCaptureError::Unsupported
    } else {
        failure(error)
    }
}
