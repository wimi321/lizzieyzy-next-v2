use super::*;

fn position(x: i32, y: i32, width: u32, height: u32) -> WindowBounds {
    WindowBounds {
        x,
        y,
        width,
        height,
    }
}

async fn apply(controller: &mut Controller<FakeWindows>, layout: WindowLayout) -> DisplaySettings {
    within(controller.action(Action::ApplyLayout(layout)))
        .await
        .unwrap()
}

#[tokio::test]
async fn explicit_layout_switches_all_three_flags_and_saves_resolved_positions() {
    let (mut controller, mut events, _directory) =
        setup(DisplaySettings::default(), FakeWindows::default()).await;
    for bits in 0..8 {
        let flags = FloatingBoardsView {
            main: bits & 4 != 0,
            keep_main_board: bits & 2 != 0,
            sub: bits & 1 != 0,
            main_on_top: true,
            sub_locked: true,
            ..Default::default()
        };
        let saved = apply(
            &mut controller,
            WindowLayout {
                main_window: Some(WindowPlacement {
                    bounds: position(1800, 900, 800, 600),
                    maximized: bits & 1 != 0,
                }),
                floating_boards: flags,
                floating_main: Some(position(20000, 100, 650, 600)),
                floating_sub: Some(position(-20000, 200, 350, 300)),
            },
        )
        .await;
        assert_eq!(saved.floating_boards, flags);
        assert_eq!(
            saved.main_window,
            Some(WindowPlacement {
                bounds: position(1120, 480, 800, 600),
                maximized: bits & 1 != 0
            })
        );
        assert_eq!(saved.floating_main, Some(position(1270, 100, 650, 600)));
        assert_eq!(saved.floating_sub, Some(position(0, 200, 350, 300)));
        assert_eq!(
            controller.platform.exists(WindowKind::FloatingMain),
            flags.main
        );
        assert_eq!(
            controller.platform.exists(WindowKind::FloatingSub),
            flags.sub
        );
        assert_eq!(
            controller.platform.windows["main"],
            saved.main_window.unwrap()
        );
        assert_eq!(
            within(controller.core.settings()).await.unwrap().display,
            saved
        );
        floating_event(&mut events, flags).await;
    }
    finish(controller).await;
}

#[tokio::test]
async fn fully_offscreen_main_layout_returns_to_primary_and_saves_fitted_position() {
    let platform = FakeWindows {
        areas: Ok(vec![
            position(0, 0, 1920, 1080),
            position(-1280, 0, 1280, 1024),
        ]),
        ..Default::default()
    };
    let (mut controller, _events, _directory) = setup(DisplaySettings::default(), platform).await;
    let requested = WindowLayout {
        main_window: Some(WindowPlacement {
            bounds: position(-20000, -20000, 900, 700),
            maximized: true,
        }),
        ..Default::default()
    };
    let saved = apply(&mut controller, requested).await;
    let expected = WindowPlacement {
        bounds: position(0, 0, 900, 700),
        maximized: true,
    };
    assert_eq!(controller.platform.windows["main"], expected);
    assert_eq!(saved.main_window, Some(expected));
    assert_eq!(
        within(controller.core.settings())
            .await
            .unwrap()
            .display
            .main_window,
        Some(expected)
    );
    finish(controller).await;
}

#[tokio::test]
async fn absent_floating_positions_use_saved_or_default_and_main_none_stays_put() {
    for has_saved in [false, true] {
        let original = DisplaySettings {
            main_window: has_saved.then_some(WindowPlacement {
                bounds: position(70, 80, 900, 700),
                maximized: false,
            }),
            floating_main: has_saved.then_some(position(500, 90, 650, 650)),
            floating_sub: has_saved.then_some(position(800, 100, 350, 350)),
            ..Default::default()
        };
        let (mut controller, _events, _directory) =
            setup(original.clone(), FakeWindows::default()).await;
        let main_before = controller.platform.windows["main"];
        let saved = apply(
            &mut controller,
            WindowLayout {
                floating_boards: FloatingBoardsView {
                    main: true,
                    sub: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;
        assert_eq!(saved.main_window, original.main_window);
        assert_eq!(controller.platform.windows["main"], main_before);
        for (kind, actual, old) in [
            (
                WindowKind::FloatingMain,
                saved.floating_main,
                original.floating_main,
            ),
            (
                WindowKind::FloatingSub,
                saved.floating_sub,
                original.floating_sub,
            ),
        ] {
            let expected =
                old.unwrap_or_else(|| window_state::default_bounds(kind, &controller.areas()));
            assert_eq!(actual, Some(expected));
            assert_eq!(
                controller.platform.windows[window_state::label(kind)].bounds,
                expected
            );
        }
        finish(controller).await;
    }
}

#[tokio::test]
async fn same_saved_layout_reapplies_after_unsaved_native_drag_and_close_captures_latest() {
    let original = DisplaySettings {
        main_window: Some(WindowPlacement {
            bounds: position(50, 60, 800, 600),
            maximized: false,
        }),
        floating_main: Some(position(900, 100, 600, 600)),
        floating_sub: Some(position(10, 100, 300, 300)),
        floating_boards: FloatingBoardsView {
            main: true,
            sub: true,
            keep_main_board: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (mut controller, _events, _directory) =
        setup(original.clone(), FakeWindows::default()).await;
    let expected = controller.platform.windows.clone();
    for placement in controller.platform.windows.values_mut() {
        placement.bounds.x += 30;
    }
    let saved = apply(&mut controller, WindowLayout::from(&original)).await;
    assert_eq!(controller.platform.windows, expected);
    assert_eq!(WindowLayout::from(&saved), WindowLayout::from(&original));
    controller
        .platform
        .windows
        .get_mut("float-main")
        .unwrap()
        .bounds
        .x += 40;
    controller
        .platform
        .windows
        .get_mut("float-sub")
        .unwrap()
        .bounds
        .y += 50;
    let last_main = controller.platform.windows["float-main"].bounds;
    let last_sub = controller.platform.windows["float-sub"].bounds;
    let closed = apply(&mut controller, WindowLayout::default()).await;
    assert_eq!(closed.floating_main, Some(last_main));
    assert_eq!(closed.floating_sub, Some(last_sub));
    assert_eq!(closed.main_window, original.main_window);
    assert!(!controller.platform.exists(WindowKind::FloatingMain));
    assert!(!controller.platform.exists(WindowKind::FloatingSub));
    finish(controller).await;
}

#[tokio::test]
async fn explicit_layout_mid_create_and_mid_place_failures_restore_all_native_state() {
    for failure in ["create", "place"] {
        let original = DisplaySettings {
            floating_boards: FloatingBoardsView {
                main: failure == "place",
                sub: failure == "place",
                ..Default::default()
            },
            ..Default::default()
        };
        let (mut controller, _events, _directory) =
            setup(original.clone(), FakeWindows::default()).await;
        let actual = controller.platform.windows.clone();
        if failure == "create" {
            controller.platform.fail_create.insert("float-sub");
        } else {
            controller.platform.fail_place_once = Some("float-sub");
        }
        let requested = WindowLayout {
            main_window: Some(WindowPlacement {
                bounds: position(10, 20, 700, 500),
                maximized: true,
            }),
            floating_boards: FloatingBoardsView {
                main: true,
                sub: true,
                main_on_top: true,
                sub_locked: true,
                ..Default::default()
            },
            floating_main: Some(position(20, 30, 450, 450)),
            floating_sub: Some(position(500, 500, 400, 400)),
        };
        assert!(
            within(controller.action(Action::ApplyLayout(requested)))
                .await
                .is_err()
        );
        assert_eq!(controller.platform.windows, actual);
        assert_eq!(
            within(controller.core.settings()).await.unwrap().display,
            original
        );
        if failure == "place" {
            assert_eq!(controller.platform.options["float-main"], (false, false));
            assert_eq!(controller.platform.options["float-sub"], (false, false));
        }
        finish(controller).await;
    }
}

#[tokio::test]
async fn explicit_layout_keeps_concurrent_ordinary_display_preferences() {
    let (mut controller, _events, _directory) =
        setup(DisplaySettings::default(), FakeWindows::default()).await;
    let core = controller.core.clone();
    let prefs = DisplaySettings {
        layout_mode: LayoutMode::Thinking,
        show_comment: false,
        board_position: 7,
        ..Default::default()
    };
    let requested = WindowLayout {
        main_window: Some(WindowPlacement {
            bounds: position(80, 90, 800, 600),
            maximized: true,
        }),
        floating_boards: FloatingBoardsView {
            main: true,
            sub: true,
            ..Default::default()
        },
        floating_main: Some(position(100, 200, 600, 600)),
        floating_sub: Some(position(800, 100, 300, 300)),
    };
    let (layout, settings) = within(async {
        tokio::join!(
            controller.action(Action::ApplyLayout(requested)),
            core.set_display_settings(prefs.clone())
        )
    })
    .await;
    layout.unwrap();
    settings.unwrap();
    let saved = within(core.settings()).await.unwrap().display;
    assert_eq!(saved.layout_mode, prefs.layout_mode);
    assert_eq!(saved.show_comment, prefs.show_comment);
    assert_eq!(saved.board_position, prefs.board_position);
    assert_eq!(WindowLayout::from(&saved), requested);
    finish(controller).await;
}

#[tokio::test]
async fn stale_display_ipc_after_debounced_native_move_has_no_window_effects() {
    use serde_json::json;
    let directory = TempDir::new().unwrap();
    let (core, subscription) = within(app_core::spawn(directory.path())).await.unwrap();
    let mut stale = within(core.settings()).await.unwrap().display;
    let shared = std::sync::Arc::new(std::sync::Mutex::new(FakeWindows::default()));
    let (requests, receiver) = mpsc::channel(64);
    let service = FloatingWindows { requests };
    let task = tokio::spawn(
        Controller {
            core: core.clone(),
            platform: SharedWindows(shared.clone()),
            placements: Placements::default(),
            reset_bounds: BTreeMap::new(),
        }
        .run(receiver),
    );
    let (send, mut events) = mpsc::unbounded_channel();
    let app = crate::commands(tauri::test::mock_builder())
        .manage(crate::DesktopState::new(core.clone(), subscription))
        .manage(service.clone())
        .channel_interceptor(move |_, _, _, body| {
            let _ = send.send(body.clone().deserialize::<AppEvent>().unwrap());
            true
        })
        .build(crate::context())
        .unwrap();
    let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let id = ipc(&main, "subscribe", json!({"channel":"__CHANNEL__:812"}))
        .await
        .unwrap();
    let requested = WindowLayout {
        main_window: Some(WindowPlacement {
            bounds: position(500, 100, 800, 600),
            maximized: false,
        }),
        floating_boards: FloatingBoardsView {
            main: true,
            keep_main_board: true,
            sub: true,
            main_on_top: true,
            sub_locked: true,
            ..Default::default()
        },
        floating_main: Some(position(1000, 200, 600, 600)),
        floating_sub: Some(position(0, 200, 300, 300)),
    };
    let applied: DisplaySettings = serde_json::from_value(
        ipc(&main, "apply_window_layout", json!({"layout":requested}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(WindowLayout::from(&applied), requested);
    tokio::time::pause();
    shared
        .lock()
        .unwrap()
        .windows
        .get_mut("main")
        .unwrap()
        .bounds
        .x = 250;
    for _ in 0..20 {
        service.changed(WindowKind::Main);
    }
    within(service.apply(Action::Layout(true, true, true)))
        .await
        .unwrap();
    within(tokio::time::advance(Duration::from_millis(500))).await;
    tokio::time::resume();
    within(async {
        loop {
            if let AppEvent::DisplaySettingsChanged(display) =
                events.recv().await.expect("event stream closed")
                && display
                    .main_window
                    .is_some_and(|placement| placement.bounds.x == 250)
            {
                break;
            }
        }
    })
    .await;
    let owned = WindowLayout::from(&within(core.settings()).await.unwrap().display);
    let native_before = shared.lock().unwrap().windows.clone();
    let effects_before = shared.lock().unwrap().effects.clone();
    stale.layout_mode = LayoutMode::Min;
    stale.board_position = 8;
    stale.show_comment = false;
    // Deliberately stale and malformed owned fields are ignored by this setter.
    stale.main_window = Some(WindowPlacement {
        bounds: position(90000, 80000, 0, 0),
        maximized: true,
    });
    stale.floating_main = Some(position(80000, 70000, 0, 0));
    stale.floating_sub = Some(position(70000, 60000, 0, 0));
    let reply: DisplaySettings = serde_json::from_value(
        ipc(&main, "set_display_settings", json!({"display":stale}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(WindowLayout::from(&reply), owned);
    assert_eq!(reply.layout_mode, LayoutMode::Min);
    assert_eq!(reply.board_position, 8);
    assert!(!reply.show_comment);
    assert_eq!(shared.lock().unwrap().windows, native_before);
    assert_eq!(shared.lock().unwrap().effects, effects_before);
    assert_eq!(within(core.settings()).await.unwrap().display, reply);
    ipc(&main, "unsubscribe", json!({"id":id})).await.unwrap();
    // The mock application retains a service clone; flush before explicitly
    // stopping this test-owned controller task instead of relying on app drop.
    within(service.apply(Action::Flush)).await.unwrap();
    task.abort();
    assert!(within(task).await.unwrap_err().is_cancelled());
    within(core.shutdown()).await.unwrap();
}
