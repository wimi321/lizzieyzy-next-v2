use super::*;
use crate::DisplaySettings;
use proptest::prelude::*;
use serde_json::json;

fn bounds(x: i32, y: i32, width: u32, height: u32) -> WindowBounds {
    WindowBounds {
        x,
        y,
        width,
        height,
    }
}

fn contains(area: WindowBounds, value: WindowBounds) -> bool {
    value.x >= area.x
        && value.y >= area.y
        && i64::from(value.x) + i64::from(value.width) <= i64::from(area.x) + i64::from(area.width)
        && i64::from(value.y) + i64::from(value.height)
            <= i64::from(area.y) + i64::from(area.height)
}

#[test]
fn layout_defaults_missing_fields_and_all_modes_round_trip() {
    let defaults = DisplaySettings::default();
    assert_eq!(defaults.layout_mode, LayoutMode::Normal);
    assert!(!defaults.large_sub_board && !defaults.large_winrate_graph);
    assert_eq!(defaults.board_position, 4);
    assert_eq!(defaults.floating_boards, FloatingBoardsView::default());
    assert_eq!(defaults.custom_layouts, [None, None]);
    assert_eq!(
        (
            defaults.main_window,
            defaults.floating_main,
            defaults.floating_sub
        ),
        (None, None, None)
    );
    assert_eq!(
        serde_json::from_value::<DisplaySettings>(json!({})).unwrap(),
        defaults
    );
    for mode in ["Normal", "Min", "Thinking", "MultiView"] {
        let display: DisplaySettings =
            serde_json::from_value(json!({"layout_mode": mode, "floating_boards": {"main": true}}))
                .unwrap();
        display.validate().unwrap();
        assert_eq!(serde_json::to_value(&display).unwrap()["layout_mode"], mode);
        assert_eq!(
            display.floating_boards,
            FloatingBoardsView {
                main: true,
                ..Default::default()
            }
        );
    }
    for invalid in [json!("Classic"), json!(0), json!(null)] {
        assert!(
            serde_json::from_value::<DisplaySettings>(json!({"layout_mode": invalid})).is_err()
        );
    }
    let custom: CustomLayout = serde_json::from_value(json!({})).unwrap();
    assert_eq!(custom, CustomLayout::default());
    assert_eq!(custom.panels, LayoutPanels::default());
}

#[test]
fn layout_validation_rejects_conflicts_and_every_invalid_geometry_coordinate() {
    for position in [0, 4, 8] {
        DisplaySettings {
            board_position: position,
            ..Default::default()
        }
        .validate()
        .unwrap();
    }
    assert!(matches!(
        DisplaySettings {
            board_position: 9,
            ..Default::default()
        }
        .validate(),
        Err(DisplaySettingsError::OutOfRange {
            field: "board_position",
            ..
        })
    ));
    assert!(matches!(
        DisplaySettings {
            large_sub_board: true,
            large_winrate_graph: true,
            ..Default::default()
        }
        .validate(),
        Err(DisplaySettingsError::ConflictingEnlargement)
    ));
    for invalid in [json!(-1), json!(1.5), json!(4_294_967_296u64)] {
        assert!(
            serde_json::from_value::<DisplaySettings>(json!({"board_position": invalid})).is_err()
        );
    }
    let valid = bounds(-100_000, 100_000, 100_000, 1);
    valid.validate().unwrap();
    for (field, value) in [
        ("x", -100_001i64),
        ("x", 100_001),
        ("y", -100_001),
        ("y", 100_001),
        ("width", 0),
        ("width", 100_001),
        ("height", 0),
        ("height", 100_001),
    ] {
        let mut raw = serde_json::to_value(valid).unwrap();
        raw[field] = value.into();
        let invalid: WindowBounds = serde_json::from_value(raw).unwrap();
        for display in [
            DisplaySettings {
                main_window: Some(WindowPlacement {
                    bounds: invalid,
                    maximized: true,
                }),
                ..Default::default()
            },
            DisplaySettings {
                floating_main: Some(invalid),
                ..Default::default()
            },
            DisplaySettings {
                floating_sub: Some(invalid),
                ..Default::default()
            },
            DisplaySettings {
                custom_layouts: [
                    Some(CustomLayout {
                        main_window: WindowPlacement {
                            bounds: invalid,
                            maximized: false,
                        },
                        ..Default::default()
                    }),
                    None,
                ],
                ..Default::default()
            },
            DisplaySettings {
                custom_layouts: [
                    None,
                    Some(CustomLayout {
                        floating_main: Some(invalid),
                        ..Default::default()
                    }),
                ],
                ..Default::default()
            },
            DisplaySettings {
                custom_layouts: [
                    Some(CustomLayout {
                        floating_sub: Some(invalid),
                        ..Default::default()
                    }),
                    None,
                ],
                ..Default::default()
            },
        ] {
            assert!(
                matches!(display.validate(), Err(DisplaySettingsError::InvalidWindowBounds { field: actual, .. }) if actual == field)
            );
        }
    }
    for custom in [
        CustomLayout {
            board_position: 9,
            ..Default::default()
        },
        CustomLayout {
            large_sub_board: true,
            large_winrate_graph: true,
            ..Default::default()
        },
    ] {
        assert!(
            DisplaySettings {
                custom_layouts: [Some(custom), None],
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    for slots in [json!([]), json!([null]), json!([null, null, null])] {
        assert!(
            serde_json::from_value::<DisplaySettings>(json!({"custom_layouts": slots})).is_err()
        );
    }
}

#[test]
fn fitting_uses_overlap_negative_monitors_clamping_and_primary_fallback() {
    let primary = bounds(0, 0, 1920, 1080);
    let left = bounds(-1280, -200, 1280, 1024);
    let screens = [primary, left];
    assert_eq!(
        fit_window_bounds(bounds(-1200, -150, 600, 600), &screens, (300, 300)),
        bounds(-1200, -150, 600, 600)
    );
    assert_eq!(
        fit_window_bounds(bounds(-200, 20, 800, 600), &screens, (300, 300)),
        bounds(0, 20, 800, 600)
    );
    assert_eq!(
        fit_window_bounds(bounds(-2000, -500, 2000, 2000), &screens, (300, 300)),
        left
    );
    assert_eq!(
        fit_window_bounds(bounds(-6000, -4000, 600, 600), &screens, (300, 300)),
        bounds(0, 0, 600, 600)
    );
    assert_eq!(
        fit_window_bounds(bounds(1900, 1000, 1, 1), &screens, (520, 400)),
        bounds(1400, 680, 520, 400)
    );
    assert_eq!(
        fit_window_bounds(bounds(0, 0, 100, 100), &[bounds(0, 0, 100, 80)], (520, 400)),
        bounds(0, 0, 100, 80)
    );
    assert_eq!(fit_window_bounds(primary, &[], (520, 400)), primary);
    assert_eq!(
        fit_window_bounds(primary, &[bounds(0, 0, 0, 0)], (520, 400)),
        primary
    );
}

#[test]
fn equal_overlap_selects_nearest_center_then_stable_monitor_order() {
    let first = bounds(0, 0, 1000, 1000);
    let second = bounds(1000, 0, 500, 1000);
    // Both intersect by 100*200, but the second monitor center is nearer.
    assert_eq!(
        fit_window_bounds(bounds(900, 100, 200, 200), &[first, second], (1, 1)),
        bounds(1000, 100, 200, 200)
    );
    let equal = bounds(1000, 0, 1000, 1000);
    assert_eq!(
        fit_window_bounds(bounds(900, 100, 200, 200), &[first, equal], (1, 1)),
        bounds(800, 100, 200, 200)
    );
}

proptest! {
    #[test]
    fn fitted_bounds_are_inside_one_work_area(
        x in -100_000i32..=100_000, y in -100_000i32..=100_000,
        width in 1u32..=100_000, height in 1u32..=100_000,
        areas in prop::collection::vec((-10_000i32..=10_000, -10_000i32..=10_000, 1u32..=8_000, 1u32..=8_000), 1..6),
        minimum in (1u32..=1000, 1u32..=1000),
    ) {
        let areas: Vec<_> = areas.into_iter().map(|(x,y,w,h)| bounds(x,y,w,h)).collect();
        let fitted = fit_window_bounds(bounds(x,y,width,height), &areas, minimum);
        prop_assert!(areas.iter().any(|area| contains(*area, fitted)));
        prop_assert!(fitted.width > 0 && fitted.height > 0);
        prop_assert_eq!(fit_window_bounds(fitted, &areas, minimum), fitted);
    }
}
