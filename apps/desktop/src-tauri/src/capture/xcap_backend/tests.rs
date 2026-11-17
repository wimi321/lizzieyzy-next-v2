#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn display_pixels_are_opaque_without_changing_rgb_or_row_order() {
    let mut rgba = [255, 0, 0, 0, 0, 255, 0, 64, 0, 0, 255, 128, 17, 34, 51, 255];
    make_opaque(&mut rgba);
    assert_eq!(
        rgba,
        [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 17, 34, 51, 255
        ]
    );
}

#[test]
fn monitor_local_dimensions_use_selected_dpi_without_reinterpreting_desktop_origins() {
    let display = NativeDisplay {
        id: 42,
        physical_bounds: Rect {
            x: -2560.0,
            y: -300.0,
            width: 2560.0,
            height: 1440.0,
        },
        scale_factor: 1.25,
    };
    assert_eq!(
        logical_bounds(&display).unwrap(),
        Rect {
            x: 0.0,
            y: 0.0,
            width: 2048.0,
            height: 1152.0
        }
    );
    assert_eq!(display.physical_bounds.x, -2560.0);
    assert_eq!(display.physical_bounds.y, -300.0);
    assert_eq!(
        logical_bounds(&NativeDisplay {
            scale_factor: 2.0,
            ..display
        })
        .unwrap()
        .width,
        1280.0
    );
    for scale_factor in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MIN_POSITIVE] {
        assert!(
            logical_bounds(&NativeDisplay {
                scale_factor,
                ..display
            })
            .is_err()
        );
    }
}

#[test]
fn only_explicit_unsupported_is_platform_unavailability() {
    assert_eq!(
        map_error(XCapError::NotSupported),
        ScreenCaptureError::Unsupported
    );
    for error in [
        XCapError::Error("permission denied".into()),
        XCapError::Error("the display was disconnected".into()),
        XCapError::InvalidCaptureRegion("outside monitor".into()),
    ] {
        let expected_reason = error.to_string();
        assert_eq!(
            map_error(error),
            ScreenCaptureError::Failed {
                reason: expected_reason
            }
        );
    }
}
