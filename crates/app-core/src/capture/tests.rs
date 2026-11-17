#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::test_support::within;
use crate::{AppEvent, AppSubscription, ColorView, EngineRules, EngineSettings, Settings};

fn image() -> CaptureImage {
    CaptureImage {
        width: 24,
        height: 24,
        rgba: [0, 0, 0, 255].repeat(24 * 24),
        scale_factor: 2.0,
    }
}

fn points() -> [CapturePoint; 3] {
    [
        CapturePoint { x: 6.0, y: 6.0 },
        CapturePoint { x: 18.0, y: 18.0 },
        CapturePoint { x: 18.0, y: 18.0 },
    ]
}

fn result() -> CaptureResult {
    CaptureResult {
        corner: CaptureCorner::BottomRight,
        cells: CaptureCells {
            rows: 2,
            columns: 3,
            cells: vec![
                CaptureCell::Black,
                CaptureCell::Empty,
                CaptureCell::White,
                CaptureCell::Empty,
                CaptureCell::Black,
                CaptureCell::Empty,
            ],
        },
        board_size: 9,
    }
}

async fn outcome(events: &mut AppSubscription) -> CaptureOutcome {
    within(async {
        loop {
            if let AppEvent::CaptureEnded { outcome } = events.recv().await.unwrap() {
                return outcome;
            }
        }
    })
    .await
}

#[tokio::test]
async fn injected_image_recognizes_with_current_thresholds() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    let session = within(core.begin_capture(image())).await.unwrap();
    assert_eq!(
        (session.width, session.height, session.scale_factor),
        (24, 24, 2.0)
    );
    let recognized = within(core.recognize_capture(session.id, points()))
        .await
        .unwrap();
    assert_eq!(
        recognized.cells,
        CaptureCells {
            rows: 2,
            columns: 2,
            cells: vec![CaptureCell::Black; 4],
        }
    );
    assert_eq!(recognized.grid.corner, CaptureCorner::TopLeft);
    assert_eq!(recognized.grid.gap_x, 12.0);
    assert_eq!(recognized.grid.gap_y, 12.0);
    let changed = CaptureSettings {
        gray_offset: 0,
        ..Default::default()
    };
    within(core.set_capture_settings(changed)).await.unwrap();
    let recognized = within(core.recognize_capture(session.id, points()))
        .await
        .unwrap();
    assert_eq!(recognized.cells.cells, vec![CaptureCell::Empty; 4]);
    assert!(matches!(
        within(core.recognize_capture(session.id, [points()[0]; 3])).await,
        Err(AppError::Capture(CaptureError::GridFit { .. }))
    ));
    // Reading or fitting must never persist the source pixels.
    let names = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(names, vec![std::ffi::OsString::from("settings.json")]);
    within(core.cancel_capture(session.id)).await.unwrap();
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn corrected_rectangular_cells_replace_document_with_black_setup_root() {
    let directory = tempfile::tempdir().unwrap();
    let (core, mut events) = within(crate::spawn(directory.path())).await.unwrap();
    within(core.open_bytes(b"(;SZ[19]PB[old];B[aa];W[bb])".to_vec()))
        .await
        .unwrap();
    within(core.set_comment("old unsaved comment".into()))
        .await
        .unwrap();
    within(core.set_engine_settings(EngineSettings {
        rules: EngineRules::Japanese,
        default_komi: 6.5,
        ..Default::default()
    }))
    .await
    .unwrap();
    let session = within(core.begin_capture(image())).await.unwrap();
    let held = within(core.capture_image(session.id)).await.unwrap();
    let weak = Arc::downgrade(&held);
    drop(held);
    let revision = within(core.finish_capture(session.id, result()))
        .await
        .unwrap();
    assert!(
        weak.upgrade().is_none(),
        "finishing must release session pixels"
    );
    let view = within(async {
        let mut document = None;
        loop {
            match events.recv().await.unwrap() {
                AppEvent::DocumentChanged {
                    revision: actual,
                    view,
                } if actual == revision => {
                    document = Some(view);
                }
                AppEvent::CaptureEnded {
                    outcome: CaptureOutcome::Finished,
                } => {
                    break document.expect("the new document must precede capture completion");
                }
                _ => {}
            }
        }
    })
    .await;
    let board = view.board.unwrap();
    assert_eq!((board.width, board.height), (9, 9));
    assert_eq!(board.to_play, ColorView::Black);
    assert_eq!(board.move_number, 0);
    assert_eq!(board.stones[7 * 9 + 6], Some(ColorView::Black));
    assert_eq!(board.stones[7 * 9 + 8], Some(ColorView::White));
    assert_eq!(board.stones[8 * 9 + 7], Some(ColorView::Black));
    assert_eq!(
        board.stones.iter().filter(|stone| stone.is_some()).count(),
        3
    );
    assert_eq!(view.game_info.komi, 6.5);
    assert_eq!(view.game_info.rules_preset, "japanese");
    assert_eq!(view.file_path, None);
    assert_eq!(view.file_name, None);
    assert!(!view.can_undo);
    let path = directory.path().join("captured.sgf");
    within(core.save(path.clone())).await.unwrap();
    let collection = sgf::parse(&std::fs::read(path).unwrap()).unwrap();
    let tree = &collection.games[0];
    assert_eq!(tree.sequence.len(), 1);
    assert!(tree.children.is_empty());
    let properties = &tree.sequence[0].properties;
    for (name, expected) in [
        ("PL", vec!["B"]),
        ("AB", vec!["gh", "hi"]),
        ("AW", vec!["ih"]),
        ("SZ", vec!["9"]),
        ("KM", vec!["6.5"]),
    ] {
        let property = properties.iter().find(|p| p.identifier == name).unwrap();
        assert_eq!(property.values, expected);
    }
    assert!(
        !properties
            .iter()
            .any(|p| matches!(p.identifier.as_str(), "B" | "W" | "PB" | "C"))
    );
    assert!(matches!(
        within(core.capture_image(session.id)).await,
        Err(AppError::Capture(CaptureError::NoSession))
    ));
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn replacement_stale_ids_and_cancel_release_only_the_correct_image() {
    let directory = tempfile::tempdir().unwrap();
    let (core, mut events) = within(crate::spawn(directory.path())).await.unwrap();
    assert!(matches!(
        within(core.capture_image(1)).await,
        Err(AppError::Capture(CaptureError::NoSession))
    ));
    let first = within(core.begin_capture(image())).await.unwrap();
    let first_image = within(core.capture_image(first.id)).await.unwrap();
    let first_weak = Arc::downgrade(&first_image);
    drop(first_image);
    let second = within(core.begin_capture(image())).await.unwrap();
    assert_ne!(first.id, second.id);
    assert!(first_weak.upgrade().is_none());
    for result in [
        within(core.cancel_capture(first.id)).await,
        within(core.finish_capture(first.id, result()))
            .await
            .map(|_| ()),
        within(core.recognize_capture(first.id, points()))
            .await
            .map(|_| ()),
        within(core.fail_capture(first.id, "old failure".into())).await,
    ] {
        assert!(matches!(
            result,
            Err(AppError::Capture(CaptureError::StaleSession))
        ));
    }
    let second_image = within(core.capture_image(second.id)).await.unwrap();
    let second_weak = Arc::downgrade(&second_image);
    drop(second_image);
    within(core.cancel_capture(second.id)).await.unwrap();
    assert!(second_weak.upgrade().is_none());
    assert_eq!(outcome(&mut events).await, CaptureOutcome::Cancelled);
    assert!(matches!(
        within(core.recognize_capture(second.id, points())).await,
        Err(AppError::Capture(CaptureError::NoSession))
    ));
    let third = within(core.begin_capture(image())).await.unwrap();
    assert!(third.id > second.id);
    within(core.fail_capture(third.id, "window failed".into()))
        .await
        .unwrap();
    assert_eq!(
        outcome(&mut events).await,
        CaptureOutcome::Failed {
            reason: "window failed".into()
        }
    );
    within(core.report_capture_failure("screen failed".into()))
        .await
        .unwrap();
    assert_eq!(
        outcome(&mut events).await,
        CaptureOutcome::Failed {
            reason: "screen failed".into()
        }
    );
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn invalid_images_do_not_replace_active_session() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    let session = within(core.begin_capture(image())).await.unwrap();
    for malformed in [
        CaptureImage {
            width: 0,
            ..image()
        },
        CaptureImage {
            width: u32::MAX,
            height: u32::MAX,
            ..image()
        },
        CaptureImage {
            rgba: vec![0; 3],
            ..image()
        },
    ] {
        assert!(matches!(
            within(core.begin_capture(malformed)).await,
            Err(AppError::Capture(CaptureError::InvalidImage))
        ));
    }
    for scale_factor in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            within(core.begin_capture(CaptureImage {
                scale_factor,
                ..image()
            }))
            .await,
            Err(AppError::Capture(CaptureError::InvalidScaleFactor))
        ));
    }
    assert_eq!(
        within(core.capture_image(session.id)).await.unwrap().rgba,
        image().rgba
    );
    within(core.cancel_capture(session.id)).await.unwrap();
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn invalid_cells_or_unrepresentable_defaults_keep_old_document_and_session() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    within(core.open_bytes(b"(;SZ[9];B[aa])".to_vec()))
        .await
        .unwrap();
    let tree = within(core.variation_tree()).await.unwrap();
    let session = within(core.begin_capture(image())).await.unwrap();
    let mut malformed = result();
    malformed.cells.cells.pop();
    assert!(matches!(
        within(core.finish_capture(session.id, malformed)).await,
        Err(AppError::Capture(CaptureError::InvalidCells))
    ));
    let too_large = CaptureResult {
        board_size: 2,
        ..result()
    };
    assert!(matches!(
        within(core.finish_capture(session.id, too_large)).await,
        Err(AppError::Capture(CaptureError::DoesNotFit))
    ));
    for board_size in [0, 1, 26, u8::MAX] {
        assert!(matches!(
            within(core.finish_capture(
                session.id,
                CaptureResult {
                    board_size,
                    ..result()
                }
            ))
            .await,
            Err(AppError::Capture(CaptureError::InvalidBoardSize))
        ));
    }
    let no_liberties = CaptureResult {
        cells: CaptureCells {
            rows: 9,
            columns: 9,
            cells: vec![CaptureCell::Black; 81],
        },
        ..result()
    };
    assert!(matches!(
        within(core.finish_capture(session.id, no_liberties)).await,
        Err(AppError::Document(_))
    ));
    within(core.set_engine_settings(EngineSettings {
        rules: EngineRules::AncientTerritory,
        ..Default::default()
    }))
    .await
    .unwrap();
    assert!(matches!(
        within(core.finish_capture(session.id, result())).await,
        Err(AppError::Capture(CaptureError::UnsupportedRules))
    ));
    assert_eq!(within(core.variation_tree()).await.unwrap(), tree);
    assert_eq!(
        within(core.capture_image(session.id)).await.unwrap().width,
        24
    );
    within(core.cancel_capture(session.id)).await.unwrap();
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn finish_respects_existing_trial_guard_and_can_retry_after_exit() {
    let directory = tempfile::tempdir().unwrap();
    let (core, mut events) = within(crate::spawn(directory.path())).await.unwrap();
    within(core.open_bytes(b"(;SZ[19];B[aa])".to_vec()))
        .await
        .unwrap();
    within(core.start_trial()).await.unwrap();
    let tree = within(core.variation_tree()).await.unwrap();
    let session = within(core.begin_capture(image())).await.unwrap();
    assert!(matches!(
        within(core.finish_capture(session.id, result())).await,
        Err(AppError::Trial(_))
    ));
    assert_eq!(within(core.variation_tree()).await.unwrap(), tree);
    assert!(within(core.capture_image(session.id)).await.is_ok());
    within(core.end_trial()).await.unwrap();
    within(core.finish_capture(session.id, result()))
        .await
        .unwrap();
    assert_eq!(outcome(&mut events).await, CaptureOutcome::Finished);
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn capture_settings_defaults_roundtrip_and_range_failures_are_atomic() {
    let directory = tempfile::tempdir().unwrap();
    for json in [r#"{"version":0}"#, r#"{"version":1,"capture":{}}"#] {
        std::fs::write(directory.path().join("settings.json"), json).unwrap();
        let loaded = within(Settings::load(directory.path())).await.unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.capture, CaptureSettings::default());
    }
    let partial: Settings = serde_json::from_str(r#"{"capture":{"black_offset":30}}"#).unwrap();
    assert_eq!(
        partial.capture,
        CaptureSettings {
            black_offset: 30,
            ..Default::default()
        }
    );
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    let chosen = CaptureSettings {
        black_offset: 0,
        black_percent: 1,
        white_offset: 255,
        white_percent: 100,
        gray_offset: 255,
    };
    assert_eq!(
        within(core.set_capture_settings(chosen)).await.unwrap(),
        chosen
    );
    assert_eq!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .capture,
        chosen
    );
    let saved = std::fs::read(directory.path().join("settings.json")).unwrap();
    for invalid in [
        CaptureSettings {
            black_offset: 256,
            ..chosen
        },
        CaptureSettings {
            white_offset: 256,
            ..chosen
        },
        CaptureSettings {
            gray_offset: 256,
            ..chosen
        },
        CaptureSettings {
            black_percent: 0,
            ..chosen
        },
        CaptureSettings {
            black_percent: 101,
            ..chosen
        },
        CaptureSettings {
            white_percent: 0,
            ..chosen
        },
        CaptureSettings {
            white_percent: 101,
            ..chosen
        },
    ] {
        assert!(matches!(
            within(core.set_capture_settings(invalid)).await,
            Err(AppError::Capture(CaptureError::InvalidSettings { .. }))
        ));
        assert_eq!(within(core.capture_settings()).await.unwrap(), chosen);
        assert_eq!(
            std::fs::read(directory.path().join("settings.json")).unwrap(),
            saved
        );
    }
    within(core.shutdown()).await.unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    assert_eq!(within(core.capture_settings()).await.unwrap(), chosen);
    within(core.shutdown()).await.unwrap();
    for json in [
        r#"{"capture":{"gray_offset":256}}"#,
        r#"{"capture":{"white_percent":0}}"#,
        r#"{"capture":{"black_offset":-1}}"#,
        r#"{"capture":{"black_offset":65536}}"#,
    ] {
        std::fs::write(directory.path().join("settings.json"), json).unwrap();
        assert!(within(Settings::load(directory.path())).await.is_err());
    }
}

#[tokio::test]
async fn failed_capture_setting_write_keeps_memory_and_original_target() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    std::fs::create_dir(directory.path().join("settings.json")).unwrap();
    let chosen = CaptureSettings {
        black_percent: 80,
        ..Default::default()
    };
    assert!(within(core.set_capture_settings(chosen)).await.is_err());
    assert_eq!(
        within(core.capture_settings()).await.unwrap(),
        CaptureSettings::default()
    );
    assert!(directory.path().join("settings.json").is_dir());
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn pending_recognition_does_not_block_actor_and_discards_replaced_or_cancelled_results() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    for replace in [true, false] {
        let session = within(core.begin_capture(image())).await.unwrap();
        let held = within(core.capture_image(session.id)).await.unwrap();
        let weak = Arc::downgrade(&held);
        drop(held);
        let (entered, ready) = oneshot::channel();
        let (release, gate) = oneshot::channel();
        let worker_core = core.clone();
        let runtime = tokio::runtime::Handle::current();
        let task = tokio::spawn(async move {
            worker_core
                .recognize_with(session.id, move |image, settings| {
                    entered.send(()).unwrap();
                    runtime.block_on(within(gate)).unwrap();
                    conversion::recognize(&image, points(), settings)
                })
                .await
        });
        within(ready).await.unwrap();
        // The worker is held at an explicit barrier, not a timing assumption.
        assert_eq!(
            within(core.capture_settings()).await.unwrap(),
            CaptureSettings::default()
        );
        if replace {
            within(core.begin_capture(image())).await.unwrap();
        } else {
            within(core.cancel_capture(session.id)).await.unwrap();
        }
        release.send(()).unwrap();
        let error = within(task).await.unwrap().unwrap_err();
        if replace {
            assert!(matches!(
                error,
                AppError::Capture(CaptureError::StaleSession)
            ));
        } else {
            assert!(matches!(error, AppError::Capture(CaptureError::NoSession)));
        }
        assert!(weak.upgrade().is_none());
    }
    within(core.shutdown()).await.unwrap();
}
