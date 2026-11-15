#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::future::Future;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("placement test timed out")
}

#[test]
fn labels_and_defaults_use_the_primary_work_area() {
    let primary = WindowBounds {
        x: -1920,
        y: -100,
        width: 1920,
        height: 1080,
    };
    let areas = [
        primary,
        WindowBounds {
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
        },
    ];
    for (window, expected) in [
        (
            WindowKind::Main,
            WindowBounds {
                x: -1600,
                y: 30,
                width: 1280,
                height: 820,
            },
        ),
        (
            WindowKind::FloatingMain,
            WindowBounds {
                x: -600,
                y: 140,
                width: 600,
                height: 600,
            },
        ),
        (
            WindowKind::FloatingSub,
            WindowBounds {
                x: -1920,
                y: 290,
                width: 300,
                height: 300,
            },
        ),
    ] {
        assert_eq!(kind(label(window)), Some(window));
        assert_eq!(default_bounds(window, &areas), expected);
    }
    assert_eq!(kind("unrelated"), None);
    assert_eq!(minimum(WindowKind::Main), (520, 400));
    let tiny = WindowBounds {
        x: 20,
        y: 30,
        width: 200,
        height: 150,
    };
    assert_eq!(default_bounds(WindowKind::Main, &[tiny]), tiny);
    assert_eq!(
        default_bounds(WindowKind::Main, &[]),
        WindowBounds {
            x: 0,
            y: 0,
            width: 1280,
            height: 820
        }
    );
}

#[tokio::test(start_paused = true)]
async fn movement_bursts_debounce_each_window_at_the_trailing_edge() {
    let mut placements = Placements::default();
    let start = Instant::now();
    placements.changed(WindowKind::Main);
    assert_eq!(placements.deadline(), Some(start + DEBOUNCE));
    within(tokio::time::advance(Duration::from_millis(400))).await;
    assert!(placements.due().is_empty());
    placements.changed(WindowKind::Main);
    placements.changed(WindowKind::FloatingMain);
    within(tokio::time::advance(Duration::from_millis(100))).await;
    assert!(placements.due().is_empty());
    placements.changed(WindowKind::FloatingMain);
    within(tokio::time::advance(Duration::from_millis(399))).await;
    assert!(placements.due().is_empty());
    within(tokio::time::advance(Duration::from_millis(1))).await;
    assert_eq!(placements.due(), vec![WindowKind::Main]);
    assert!(placements.due().is_empty());
    within(tokio::time::advance(Duration::from_millis(100))).await;
    assert_eq!(placements.due(), vec![WindowKind::FloatingMain]);
    assert_eq!(placements.deadline(), None);
    placements.changed(WindowKind::FloatingSub);
    placements.clear();
    within(tokio::time::advance(DEBOUNCE)).await;
    assert!(placements.due().is_empty());
    assert_eq!(placements.deadline(), None);
}

#[test]
fn maximize_keeps_normal_restore_bounds_separately_for_each_window() {
    let mut placements = Placements::default();
    let normal = WindowBounds {
        x: -500,
        y: 100,
        width: 600,
        height: 450,
    };
    let maximized = WindowPlacement {
        bounds: WindowBounds {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
        },
        maximized: true,
    };
    placements.seed(WindowKind::Main, normal);
    assert_eq!(
        placements.observe(WindowKind::Main, maximized),
        WindowPlacement {
            bounds: normal,
            maximized: true
        }
    );
    assert_eq!(
        placements.observe(WindowKind::FloatingSub, maximized),
        maximized
    );
    let resized = WindowPlacement {
        bounds: WindowBounds {
            width: 700,
            ..normal
        },
        maximized: false,
    };
    assert_eq!(placements.observe(WindowKind::Main, resized), resized);
    assert_eq!(
        placements.observe(WindowKind::Main, maximized).bounds,
        resized.bounds
    );
}
