//! Pure display selection and screenshot coordinate conversion.
use super::ScreenCaptureError;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .into_iter()
            .all(f64::is_finite)
            && self.width > 0.0
            && self.height > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Display {
    // CoreGraphics and XCap both retain the selected native display identity.
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    pub id: u32,
    pub bounds: Rect,
}

/// All rectangles share one desktop coordinate system (logical on macOS,
/// physical on Windows), including negative origins. Gaps are rejected.
pub(super) fn select_display(
    main: Rect,
    displays: &[Display],
) -> Result<Display, ScreenCaptureError> {
    if !main.valid() {
        return Err(ScreenCaptureError::DisplayUnavailable);
    }
    let center = (main.x + main.width / 2.0, main.y + main.height / 2.0);
    displays
        .iter()
        .copied()
        .find(|display| {
            let bounds = display.bounds;
            bounds.valid()
                && center.0 >= bounds.x
                && center.1 >= bounds.y
                && center.0 < bounds.x + bounds.width
                && center.1 < bounds.y + bounds.height
        })
        .ok_or(ScreenCaptureError::DisplayUnavailable)
}

/// Use the image's real dimensions instead of assuming the OS scale matches
/// the returned screenshot. Coordinates are local to the frozen capture page.
pub(super) fn logical_to_pixel(
    x: f64,
    y: f64,
    display: Rect,
    width: u32,
    height: u32,
) -> Result<(f64, f64), ScreenCaptureError> {
    if !display.valid()
        || width == 0
        || height == 0
        || !x.is_finite()
        || !y.is_finite()
        || x < 0.0
        || y < 0.0
        || x > display.width
        || y > display.height
    {
        return Err(ScreenCaptureError::DisplayUnavailable);
    }
    Ok((
        x * f64::from(width) / display.width,
        y * f64::from(height) / display.height,
    ))
}

pub(super) fn image_scale(
    display: Rect,
    width: u32,
    height: u32,
) -> Result<f64, ScreenCaptureError> {
    let pixel = logical_to_pixel(display.width, display.height, display, width, height)?;
    Ok(pixel.0 / display.width)
}
