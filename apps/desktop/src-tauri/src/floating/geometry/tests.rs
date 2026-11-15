use super::*;

fn bounds(x: i32, y: i32, width: u32, height: u32) -> WindowBounds {
    WindowBounds {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn restore_from_retina_primary_to_standard_secondary_uses_destination_dpi() {
    let screens = [
        (bounds(0, 0, 1920, 1080), 2.0),
        (bounds(3840, 0, 1920, 1080), 1.0),
    ];
    let (position, size) = target_geometry(bounds(4000, 100, 900, 700), (16, 40), &screens, 2.0);
    assert_eq!(position, PhysicalPosition::new(4000, 100));
    assert_eq!(size, PhysicalSize::new(884, 660));
}

#[test]
fn negative_secondary_and_return_to_retina_convert_position_and_client_size() {
    let screens = [
        (bounds(0, 0, 1920, 1080), 2.0),
        (bounds(-1920, 0, 1920, 1080), 1.0),
    ];
    let (position, size) = target_geometry(bounds(-1800, 200, 600, 600), (0, 0), &screens, 2.0);
    assert_eq!(position, PhysicalPosition::new(-1800, 200));
    assert_eq!(size, PhysicalSize::new(600, 600));
    let (position, size) = target_geometry(bounds(100, 200, 900, 700), (16, 40), &screens, 1.0);
    assert_eq!(position, PhysicalPosition::new(200, 400));
    assert_eq!(size, PhysicalSize::new(1768, 1320));
}

#[test]
fn overlapping_logical_areas_choose_the_nearest_center_for_equal_overlap() {
    let screens = [
        (bounds(0, 0, 1920, 1080), 1.0),
        (bounds(960, 0, 1920, 1080), 2.0),
    ];
    let (position, _) = target_geometry(bounds(1500, 300, 300, 300), (0, 0), &screens, 1.0);
    assert_eq!(position, PhysicalPosition::new(3000, 600));
}

#[test]
fn missing_monitors_use_current_dpi_and_invalid_dpi_cannot_panic() {
    let value = bounds(100, 200, 600, 400);
    let (position, size) = target_geometry(value, (0, 0), &[], 1.5);
    assert_eq!(position, PhysicalPosition::new(150, 300));
    assert_eq!(size, PhysicalSize::new(900, 600));
    let (position, size) = target_geometry(value, (700, 500), &[(value, f64::NAN)], 0.0);
    assert_eq!(position, PhysicalPosition::new(100, 200));
    assert_eq!(size, PhysicalSize::new(1, 1));
}
