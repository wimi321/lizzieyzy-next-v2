//! Only pure sizing/error mapping: never enumerate displays or capture screens.

use super::{SCError, ScreenCaptureError, capture_error, pixel_dimensions};

#[test]
fn filter_points_preserve_retina_pixels_and_round_fractional_sizes() {
    assert_eq!(pixel_dimensions(2048.0, 1152.0, 2.0), Ok((4096, 2304)));
    assert_eq!(pixel_dimensions(1920.0, 1080.0, 1.0), Ok((1920, 1080)));
    assert_eq!(pixel_dimensions(100.4, 90.0, 1.25), Ok((126, 113)));
}

#[test]
fn invalid_filter_geometry_fails_before_any_image_allocation() {
    for (width, height, scale) in [
        (0.0, 1080.0, 1.0),
        (-1.0, 1080.0, 1.0),
        (1920.0, 0.0, 1.0),
        (f64::NAN, 1080.0, 1.0),
        (1920.0, f64::INFINITY, 1.0),
        (1920.0, 1080.0, 0.0),
        (1920.0, 1080.0, -1.0),
        (1920.0, 1080.0, f64::NAN),
        (1920.0, 1080.0, f64::INFINITY),
        (f64::from(u32::MAX) + 1.0, 1.0, 1.0),
        (f64::MAX, f64::MAX, 2.0),
    ] {
        assert!(matches!(
            pixel_dimensions(width, height, scale),
            Err(ScreenCaptureError::Failed { reason }) if !reason.is_empty()
        ));
    }
    assert_eq!(pixel_dimensions(8192.0, 4096.0, 1.0), Ok((8192, 4096)));
    assert!(matches!(
        pixel_dimensions(8192.0, 4097.0, 1.0),
        Err(ScreenCaptureError::Failed { .. })
    ));
}

#[test]
fn only_explicit_feature_unavailability_maps_to_unsupported() {
    assert_eq!(
        capture_error(SCError::feature_not_available(
            "SCContentFilter.style",
            "14.0"
        )),
        ScreenCaptureError::Unsupported
    );
    for error in [
        SCError::ScreenshotError("native capture failed".into()),
        SCError::PermissionDenied("screen permission was revoked".into()),
        SCError::NoShareableContent("display disconnected".into()),
        SCError::InvalidConfiguration("bad pixel format".into()),
    ] {
        let reason = error.to_string();
        assert_eq!(capture_error(error), ScreenCaptureError::Failed { reason });
    }
}
