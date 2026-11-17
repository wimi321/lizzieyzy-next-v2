//! Windows-only adapter to xcap's safe monitor API.
//!
//! xcap 0.9.10 uses EnumDisplaySettingsW for monitor origins and dimensions:
//! these are physical desktop pixels, not per-monitor logical coordinates.
//! The shell must select against the physical main-window center before using
//! the selected monitor's scale for its capture page's logical dimensions.

use app_core::CaptureImage;
use xcap::{Monitor, XCapError};

use super::{ScreenCaptureError, failure, geometry::Rect};

#[derive(Clone, Copy, Debug)]
pub(super) struct NativeDisplay {
    pub id: u32,
    pub physical_bounds: Rect,
    pub scale_factor: f64,
}

/// Native enumeration only; automated tests must use the injected Platform.
pub(super) fn displays() -> Result<Vec<NativeDisplay>, ScreenCaptureError> {
    Monitor::all()
        .map_err(map_error)?
        .iter()
        .map(describe)
        .collect()
}

/// Re-resolve native geometry for the already selected monitor only.
pub(super) fn display(id: u32) -> Result<NativeDisplay, ScreenCaptureError> {
    displays()?
        .into_iter()
        .find(|display| display.id == id)
        .ok_or_else(|| failure("the selected display is no longer available"))
}

/// Capture-local logical dimensions. Selection and native window placement must
/// continue using physical_bounds; dividing every desktop origin by a different
/// monitor scale would create gaps/overlaps on a mixed-DPI Windows desktop.
pub(super) fn logical_bounds(display: &NativeDisplay) -> Result<Rect, ScreenCaptureError> {
    let scale = display.scale_factor;
    let width = display.physical_bounds.width / scale;
    let height = display.physical_bounds.height / scale;
    if !scale.is_finite()
        || scale <= 0.0
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        return Err(failure("the display returned invalid dimensions or scale"));
    }
    Ok(Rect {
        x: 0.0,
        y: 0.0,
        width,
        height,
    })
}

fn describe(monitor: &Monitor) -> Result<NativeDisplay, ScreenCaptureError> {
    let scale_factor = f64::from(monitor.scale_factor().map_err(map_error)?);
    let width = monitor.width().map_err(map_error)?;
    let height = monitor.height().map_err(map_error)?;
    if !scale_factor.is_finite() || scale_factor <= 0.0 || width == 0 || height == 0 {
        return Err(failure("the display returned invalid dimensions or scale"));
    }
    Ok(NativeDisplay {
        id: monitor.id().map_err(map_error)?,
        physical_bounds: Rect {
            x: f64::from(monitor.x().map_err(map_error)?),
            y: f64::from(monitor.y().map_err(map_error)?),
            width: f64::from(width),
            height: f64::from(height),
        },
        scale_factor,
    })
}

/// Called on the capture worker after the host windows have been hidden.
/// The monitor ID is re-resolved so disconnecting a display is a typed failure,
/// never a silent switch to a different display or the entire virtual desktop.
pub(super) fn capture(id: u32) -> Result<CaptureImage, ScreenCaptureError> {
    let monitors = Monitor::all().map_err(map_error)?;
    let mut selected = None;
    for monitor in monitors {
        if monitor.id().map_err(map_error)? == id {
            selected = Some(monitor);
            break;
        }
    }
    let monitor = selected.ok_or_else(|| failure("the selected display is no longer available"))?;
    let info = describe(&monitor)?;
    let pixel_count = u64::from(monitor.width().map_err(map_error)?)
        .checked_mul(u64::from(monitor.height().map_err(map_error)?))
        .and_then(|pixels| pixels.checked_mul(4));
    if pixel_count.is_none_or(|bytes| bytes > app_core::capture::MAX_CAPTURE_BYTES as u64) {
        return Err(failure("screenshot dimensions exceed the memory limit"));
    }
    // Do not save, encode, resize, or derive dimensions from OS DPI here.
    // capture_image returns a row-major RGBA8 buffer of the actual display.
    let image = monitor.capture_image().map_err(map_error)?;
    let (width, height) = image.dimensions();
    let mut rgba = image.into_raw();
    make_opaque(&mut rgba);
    let image = CaptureImage {
        width,
        height,
        rgba,
        scale_factor: info.scale_factor,
    };
    image.validate().map_err(failure)?;
    Ok(image)
}

/// GDI already supplies RGB, but its unused alpha byte is not guaranteed to be
/// opaque. A display screenshot has no transparent background; keep its colors
/// unchanged when encoding PNG or passing the pixels to recognition.
fn make_opaque(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
}

/// Only the library's explicit unsupported variant is platform unavailability.
/// Permission, monitor, allocation and capture failures retain their reason.
pub(super) fn map_error(error: XCapError) -> ScreenCaptureError {
    match error {
        XCapError::NotSupported => ScreenCaptureError::Unsupported,
        other => failure(other),
    }
}

#[cfg(test)]
mod tests;
