#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use app_core::{AppEvent, AppSubscription, LayoutMode, WindowLayout};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    time::Duration,
};
use tempfile::TempDir;

mod layout;
mod startup;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("floating window test timed out")
}

#[derive(Clone, Debug, PartialEq)]
enum Effect {
    Create(WindowKind, WindowBounds, bool, bool),
    Destroy(WindowKind),
    Place(WindowKind, WindowPlacement),
    Options(FloatingBoardKind, bool, bool),
}

struct FakeWindows {
    areas: Result<Vec<WindowBounds>, String>,
    frames: BTreeMap<&'static str, (u32, u32)>,
    windows: BTreeMap<&'static str, WindowPlacement>,
    options: BTreeMap<&'static str, (bool, bool)>,
    effects: Vec<Effect>,
    fail_create: BTreeSet<&'static str>,
    fail_place_once: Option<&'static str>,
    fail_placement: BTreeSet<&'static str>,
    missing_placement: BTreeSet<&'static str>,
}

impl Default for FakeWindows {
    fn default() -> Self {
        Self {
            areas: Ok(vec![WindowBounds {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }]),
            windows: [(
                "main",
                WindowPlacement {
                    bounds: WindowBounds {
                        x: 320,
                        y: 130,
                        width: 1280,
                        height: 820,
                    },
                    maximized: false,
                },
            )]
            .into(),
            options: BTreeMap::new(),
            frames: BTreeMap::new(),
            effects: Vec::new(),
            fail_create: BTreeSet::new(),
            fail_place_once: None,
            fail_placement: BTreeSet::new(),
            missing_placement: BTreeSet::new(),
        }
    }
}

impl WindowPlatform for FakeWindows {
    fn work_areas(&self) -> Result<Vec<WindowBounds>, String> {
        self.areas.clone()
    }
    fn frame(&self, kind: WindowKind) -> (u32, u32) {
        self.frames
            .get(window_state::label(kind))
            .copied()
            .unwrap_or_default()
    }
    fn exists(&self, kind: WindowKind) -> bool {
        self.windows.contains_key(window_state::label(kind))
    }
    fn create(
        &mut self,
        kind: FloatingBoardKind,
        bounds: WindowBounds,
        top: bool,
        locked: bool,
    ) -> Result<(), String> {
        let target = window_kind(kind);
        let label = window_state::label(target);
        if self.fail_create.contains(label) {
            return Err("injected create failure".into());
        }
        assert!(
            !self.windows.contains_key(label),
            "duplicate native window creation"
        );
        self.windows.insert(
            label,
            WindowPlacement {
                bounds,
                maximized: false,
            },
        );
        self.options.insert(label, (top, locked));
        self.effects
            .push(Effect::Create(target, bounds, top, locked));
        Ok(())
    }
    fn destroy(&mut self, kind: WindowKind) -> Result<(), String> {
        self.windows.remove(window_state::label(kind));
        self.effects.push(Effect::Destroy(kind));
        Ok(())
    }
    fn placement(&self, kind: WindowKind) -> Result<Option<WindowPlacement>, String> {
        let label = window_state::label(kind);
        if self.fail_placement.contains(label) {
            return Err("injected placement read failure".into());
        }
        if self.missing_placement.contains(label) {
            return Ok(None);
        }
        Ok(self.windows.get(label).copied())
    }
    fn place(&mut self, kind: WindowKind, placement: WindowPlacement) -> Result<(), String> {
        if self.fail_place_once == Some(window_state::label(kind)) {
            self.fail_place_once = None;
            return Err("injected place failure".into());
        }
        self.windows.insert(window_state::label(kind), placement);
        self.effects.push(Effect::Place(kind, placement));
        Ok(())
    }
    fn options(&mut self, kind: FloatingBoardKind, top: bool, locked: bool) -> Result<(), String> {
        self.options
            .insert(window_state::label(window_kind(kind)), (top, locked));
        self.effects.push(Effect::Options(kind, top, locked));
        Ok(())
    }
}

async fn setup(
    display: DisplaySettings,
    platform: FakeWindows,
) -> (Controller<FakeWindows>, AppSubscription, TempDir) {
    let directory = TempDir::new().unwrap();
    let (core, subscription) = within(app_core::spawn(directory.path())).await.unwrap();
    let layout = WindowLayout::from(&display);
    within(core.set_display_settings(display)).await.unwrap();
    within(core.save_window_layout(layout)).await.unwrap();
    let mut controller = Controller {
        core,
        platform,
        placements: Placements::default(),
        reset_bounds: BTreeMap::new(),
    };
    within(controller.initialize()).await;
    (controller, subscription, directory)
}

async fn floating_event(subscription: &mut AppSubscription, expected: FloatingBoardsView) {
    within(async {
        loop {
            if let AppEvent::FloatingBoardsChanged(view) =
                subscription.recv().await.expect("core stopped")
                && view == expected
            {
                return;
            }
        }
    })
    .await;
}

async fn finish(controller: Controller<FakeWindows>) {
    within(controller.core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn all_eight_layout_combinations_keep_the_three_flags_independent() {
    let (mut controller, mut events, _directory) =
        setup(DisplaySettings::default(), FakeWindows::default()).await;
    for bits in 0..8 {
        let (main, keep, sub) = (bits & 4 != 0, bits & 2 != 0, bits & 1 != 0);
        let saved = within(controller.action(Action::Layout(main, keep, sub)))
            .await
            .unwrap();
        assert_eq!(
            (
                saved.floating_boards.main,
                saved.floating_boards.keep_main_board,
                saved.floating_boards.sub
            ),
            (main, keep, sub)
        );
        assert_eq!(controller.platform.exists(WindowKind::FloatingMain), main);
        assert_eq!(controller.platform.exists(WindowKind::FloatingSub), sub);
        if bits != 0 {
            floating_event(&mut events, saved.floating_boards).await;
        }
    }
    finish(controller).await;
}

#[tokio::test]
async fn repeat_open_options_and_system_close_preserve_layout_and_other_window() {
    let display = DisplaySettings {
        layout_mode: LayoutMode::Thinking,
        ..Default::default()
    };
    let (mut controller, mut events, _directory) = setup(display, FakeWindows::default()).await;
    within(controller.action(Action::Layout(true, true, true)))
        .await
        .unwrap();
    within(controller.action(Action::Open(FloatingBoardKind::Main, true)))
        .await
        .unwrap();
    assert_eq!(
        controller
            .platform
            .effects
            .iter()
            .filter(|effect| matches!(effect, Effect::Create(WindowKind::FloatingMain, ..)))
            .count(),
        1
    );
    let saved = within(controller.action(Action::Options(FloatingBoardKind::Main, true, true)))
        .await
        .unwrap();
    assert_eq!(controller.platform.options["float-main"], (true, true));
    floating_event(&mut events, saved.floating_boards).await;
    let moved = WindowPlacement {
        bounds: WindowBounds {
            x: 900,
            y: 60,
            width: 640,
            height: 650,
        },
        maximized: false,
    };
    controller.platform.windows.insert("float-main", moved);
    let closed = within(controller.action(native::close_action(WindowKind::FloatingMain)))
        .await
        .unwrap();
    assert!(!closed.floating_boards.main);
    assert!(closed.floating_boards.keep_main_board && closed.floating_boards.sub);
    assert!(closed.floating_boards.main_on_top && closed.floating_boards.main_locked);
    assert_eq!(closed.layout_mode, LayoutMode::Thinking);
    assert_eq!(closed.floating_main, Some(moved.bounds));
    assert!(!controller.platform.exists(WindowKind::FloatingMain));
    assert!(controller.platform.exists(WindowKind::FloatingSub));
    floating_event(&mut events, closed.floating_boards).await;
    let reopened = within(controller.action(Action::Open(FloatingBoardKind::Main, true)))
        .await
        .unwrap();
    assert_eq!(controller.platform.windows["float-main"], moved);
    assert_eq!(controller.platform.options["float-main"], (true, true));
    floating_event(&mut events, reopened.floating_boards).await;
    let closed_sub = within(controller.action(native::close_action(WindowKind::FloatingSub)))
        .await
        .unwrap();
    assert!(closed_sub.floating_boards.main && closed_sub.floating_boards.keep_main_board);
    assert!(!closed_sub.floating_boards.sub);
    floating_event(&mut events, closed_sub.floating_boards).await;
    finish(controller).await;
}

#[tokio::test]
async fn startup_restores_fitted_bounds_maximize_and_floating_options() {
    let primary = WindowBounds {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };
    let secondary = WindowBounds {
        x: -1280,
        y: 0,
        width: 1280,
        height: 1024,
    };
    let platform = FakeWindows {
        areas: Ok(vec![primary, secondary]),
        ..Default::default()
    };
    let display = DisplaySettings {
        main_window: Some(WindowPlacement {
            bounds: WindowBounds {
                x: -1200,
                y: -100,
                width: 1400,
                height: 1200,
            },
            maximized: true,
        }),
        floating_main: Some(WindowBounds {
            x: 10000,
            y: 400,
            width: 700,
            height: 600,
        }),
        floating_sub: Some(WindowBounds {
            x: -900,
            y: 50,
            width: 300,
            height: 300,
        }),
        floating_boards: FloatingBoardsView {
            main: true,
            sub: true,
            main_on_top: true,
            sub_locked: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (controller, _events, _directory) = setup(display, platform).await;
    assert_eq!(
        controller.platform.windows["main"],
        WindowPlacement {
            bounds: secondary,
            maximized: true
        }
    );
    assert_eq!(
        controller.platform.windows["float-main"].bounds,
        WindowBounds {
            x: 1220,
            y: 400,
            width: 700,
            height: 600
        }
    );
    assert_eq!(
        controller.platform.windows["float-sub"].bounds,
        WindowBounds {
            x: -900,
            y: 50,
            width: 300,
            height: 300
        }
    );
    assert_eq!(controller.platform.options["float-main"], (true, false));
    assert_eq!(controller.platform.options["float-sub"], (false, true));
    finish(controller).await;
}

#[tokio::test]
async fn optional_monitor_and_floating_creation_failures_keep_main_usable() {
    let platform = FakeWindows {
        areas: Err("no displays".into()),
        fail_create: ["float-main"].into(),
        ..Default::default()
    };
    let original_main = platform.windows["main"];
    let display = DisplaySettings {
        main_window: Some(WindowPlacement {
            bounds: WindowBounds {
                x: 90000,
                y: 80000,
                width: 1000,
                height: 800,
            },
            maximized: true,
        }),
        floating_boards: FloatingBoardsView {
            main: true,
            keep_main_board: true,
            sub: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (mut controller, mut events, _directory) = setup(display, platform).await;
    assert_eq!(controller.platform.windows["main"], original_main);
    assert!(!controller.platform.exists(WindowKind::FloatingMain));
    assert!(controller.platform.exists(WindowKind::FloatingSub));
    let settings = within(controller.core.settings()).await.unwrap();
    assert_eq!(
        settings.display.floating_boards,
        FloatingBoardsView {
            keep_main_board: true,
            sub: true,
            ..Default::default()
        }
    );
    floating_event(&mut events, settings.display.floating_boards).await;
    assert!(
        within(controller.action(Action::Open(FloatingBoardKind::Main, true)))
            .await
            .is_err()
    );
    assert_eq!(
        within(controller.core.settings())
            .await
            .unwrap()
            .display
            .floating_boards,
        settings.display.floating_boards
    );
    finish(controller).await;
}

#[tokio::test]
async fn reset_is_immediate_preserves_preferences_and_stays_cleared_until_a_user_move() {
    let original = DisplaySettings {
        layout_mode: LayoutMode::Min,
        board_position: 8,
        show_comment: false,
        show_sub_board: false,
        main_window: Some(WindowPlacement {
            bounds: WindowBounds {
                x: 50,
                y: 30,
                width: 800,
                height: 600,
            },
            maximized: true,
        }),
        floating_main: Some(WindowBounds {
            x: 100,
            y: 20,
            width: 400,
            height: 400,
        }),
        floating_sub: Some(WindowBounds {
            x: 100,
            y: 20,
            width: 200,
            height: 200,
        }),
        floating_boards: FloatingBoardsView {
            main: true,
            keep_main_board: true,
            sub: true,
            main_locked: true,
            sub_on_top: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (mut controller, _events, _directory) =
        setup(original.clone(), FakeWindows::default()).await;
    let reset = within(controller.action(Action::Reset)).await.unwrap();
    let expected = DisplaySettings {
        main_window: None,
        floating_main: None,
        floating_sub: None,
        board_position: 4,
        ..original
    };
    assert_eq!(reset, expected);
    for kind in [
        WindowKind::Main,
        WindowKind::FloatingMain,
        WindowKind::FloatingSub,
    ] {
        assert_eq!(
            controller.platform.windows[window_state::label(kind)],
            WindowPlacement {
                bounds: window_state::default_bounds(kind, &controller.areas()),
                maximized: false,
            }
        );
    }
    assert_eq!(
        within(controller.action(Action::Flush)).await.unwrap(),
        expected
    );
    let mut moved = controller.platform.windows["main"];
    moved.bounds.x += 20;
    controller.platform.windows.insert("main", moved);
    let saved = within(controller.action(Action::Flush)).await.unwrap();
    assert_eq!(saved.main_window, Some(moved));
    assert_eq!(saved.floating_main, None);
    assert_eq!(saved.floating_sub, None);
    assert_eq!(saved.floating_boards, expected.floating_boards);
    finish(controller).await;
}

#[tokio::test]
async fn fit_keeps_top_left_clamps_size_and_does_nothing_when_maximized() {
    let (mut controller, _events, _directory) =
        setup(DisplaySettings::default(), FakeWindows::default()).await;
    let fitted = within(controller.action(Action::Fit(520, 400)))
        .await
        .unwrap();
    assert_eq!(
        fitted.main_window.unwrap().bounds,
        WindowBounds {
            x: 320,
            y: 130,
            width: 520,
            height: 400
        }
    );
    let fitted = within(controller.action(Action::Fit(4000, 4000)))
        .await
        .unwrap();
    assert_eq!(
        fitted.main_window.unwrap().bounds,
        WindowBounds {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080
        }
    );
    controller
        .platform
        .windows
        .get_mut("main")
        .unwrap()
        .maximized = true;
    let count = controller.platform.effects.len();
    assert_eq!(
        within(controller.action(Action::Fit(800, 600)))
            .await
            .unwrap(),
        fitted
    );
    assert_eq!(controller.platform.effects.len(), count);
    assert!(
        within(controller.action(Action::Fit(0, 600)))
            .await
            .is_err()
    );
    assert!(
        within(controller.action(Action::Fit(800, 100001)))
            .await
            .is_err()
    );
    assert_eq!(controller.platform.effects.len(), count);
    finish(controller).await;
}

#[tokio::test]
async fn changed_layout_placements_are_fitted_and_applied_to_already_open_windows() {
    let display = DisplaySettings {
        floating_boards: FloatingBoardsView {
            main: true,
            sub: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (mut controller, _events, _directory) =
        setup(display.clone(), FakeWindows::default()).await;
    let requested = DisplaySettings {
        main_window: Some(WindowPlacement {
            bounds: WindowBounds {
                x: 1500,
                y: 900,
                width: 800,
                height: 600,
            },
            maximized: true,
        }),
        floating_main: Some(WindowBounds {
            x: -5000,
            y: 100,
            width: 650,
            height: 650,
        }),
        floating_sub: Some(WindowBounds {
            x: 1800,
            y: -100,
            width: 400,
            height: 400,
        }),
        ..display
    };
    within(controller.action(Action::ApplyLayout(WindowLayout::from(&requested))))
        .await
        .unwrap();
    assert_eq!(
        controller.platform.windows["main"],
        WindowPlacement {
            bounds: WindowBounds {
                x: 1120,
                y: 480,
                width: 800,
                height: 600
            },
            maximized: true
        }
    );
    assert_eq!(
        controller.platform.windows["float-main"].bounds,
        WindowBounds {
            x: 0,
            y: 100,
            width: 650,
            height: 650
        }
    );
    assert_eq!(
        controller.platform.windows["float-sub"].bounds,
        WindowBounds {
            x: 1520,
            y: 0,
            width: 400,
            height: 400
        }
    );
    assert_eq!(
        controller
            .platform
            .effects
            .iter()
            .filter(|effect| matches!(effect, Effect::Create(..)))
            .count(),
        2
    );
    finish(controller).await;
}

#[test]
fn floating_app_urls_resolve_the_query_for_development_and_packaged_frontends() {
    for (kind, label) in [
        (FloatingBoardKind::Main, "float-main"),
        (FloatingBoardKind::Sub, "float-sub"),
    ] {
        let tauri::WebviewUrl::App(path) = native::floating_url(kind) else {
            panic!("floating windows must use app URLs");
        };
        for base in [
            "http://localhost:1420/",
            "tauri://localhost/",
            "http://tauri.localhost/",
        ] {
            let base: tauri::Url = base.parse().unwrap();
            let joined = base.join(&path.to_string_lossy()).unwrap();
            assert_eq!(joined.query(), Some(format!("window={label}").as_str()));
            assert_eq!(joined.host_str(), base.host_str());
            assert_eq!(joined.path(), "/index.html");
        }
    }
}

async fn ipc(
    window: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    command: &str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, serde_json::Value> {
    use tauri::{
        ipc::{CallbackFn, InvokeBody, InvokeResponse},
        webview::InvokeRequest,
    };
    let (send, receive) = oneshot::channel();
    let url = if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    };
    window.as_ref().clone().on_message(
        InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: url.parse().unwrap(),
            body: InvokeBody::Json(arguments),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
        Box::new(move |_, _, response, _, _| {
            let _ = send.send(response);
        }),
    );
    match within(receive).await.unwrap() {
        InvokeResponse::Ok(body) => Ok(body.deserialize().unwrap()),
        InvokeResponse::Err(error) => Err(error.0),
    }
}

#[tokio::test]
async fn mock_runtime_window_ipc_enforces_self_options_and_close() {
    use serde_json::json;
    let directory = TempDir::new().unwrap();
    let (core, subscription) = within(app_core::spawn(directory.path())).await.unwrap();
    let windows = FloatingWindows::spawn(FakeWindows::default(), core.clone());
    let app = crate::commands(tauri::test::mock_builder())
        .manage(crate::DesktopState::new(core.clone(), subscription))
        .manage(windows.clone())
        .build(crate::context())
        .unwrap();
    let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let floating = tauri::WebviewWindowBuilder::new(&app, "float-main", Default::default())
        .build()
        .unwrap();
    let opened = ipc(&main, "open_floating_board", json!({"kind":"Main"}))
        .await
        .unwrap();
    assert_eq!(opened["main"], true);
    let options = ipc(
        &floating,
        "set_floating_board_options",
        json!({"kind":"Main","onTop":true,"locked":true}),
    )
    .await
    .unwrap();
    assert_eq!(options["main_on_top"], true);
    assert_eq!(options["main_locked"], true);
    for (command, args) in [
        ("close_floating_board", json!({"kind":"Sub"})),
        (
            "set_floating_board_options",
            json!({"kind":"Sub","onTop":true,"locked":true}),
        ),
    ] {
        let error = ipc(&floating, command, args).await.unwrap_err();
        assert!(
            error.to_string().contains("only change its own window"),
            "{error}"
        );
    }
    let closed = ipc(&floating, "close_floating_board", json!({"kind":"Main"}))
        .await
        .unwrap();
    assert_eq!(closed["main"], false);
    assert_eq!(closed["main_locked"], true);
    let layout = ipc(
        &main,
        "set_floating_layout",
        json!({"main":true,"keepMainBoard":true,"sub":true}),
    )
    .await
    .unwrap();
    assert_eq!(layout["main"], true);
    assert_eq!(layout["keep_main_board"], true);
    assert_eq!(layout["sub"], true);
    ipc(&main, "fit_main_window", json!({"width":700,"height":500}))
        .await
        .unwrap();
    assert_eq!(
        within(core.settings())
            .await
            .unwrap()
            .display
            .main_window
            .unwrap()
            .bounds
            .width,
        700
    );
    ipc(&main, "reset_window_positions", json!({}))
        .await
        .unwrap();
    let reset = within(core.settings()).await.unwrap().display;
    assert_eq!(reset.main_window, None);
    assert_eq!(reset.floating_main, None);
    assert_eq!(reset.floating_sub, None);
    assert!(
        reset.floating_boards.main
            && reset.floating_boards.sub
            && reset.floating_boards.keep_main_board
    );
    within(windows.apply(Action::Flush)).await.unwrap();
    within(core.shutdown()).await.unwrap();
}

struct SharedWindows(std::sync::Arc<std::sync::Mutex<FakeWindows>>);

impl WindowPlatform for SharedWindows {
    fn work_areas(&self) -> Result<Vec<WindowBounds>, String> {
        self.0.lock().unwrap().work_areas()
    }
    fn frame(&self, kind: WindowKind) -> (u32, u32) {
        self.0.lock().unwrap().frame(kind)
    }
    fn exists(&self, kind: WindowKind) -> bool {
        self.0.lock().unwrap().exists(kind)
    }
    fn create(
        &mut self,
        kind: FloatingBoardKind,
        bounds: WindowBounds,
        top: bool,
        locked: bool,
    ) -> Result<(), String> {
        self.0.lock().unwrap().create(kind, bounds, top, locked)
    }
    fn destroy(&mut self, kind: WindowKind) -> Result<(), String> {
        self.0.lock().unwrap().destroy(kind)
    }
    fn placement(&self, kind: WindowKind) -> Result<Option<WindowPlacement>, String> {
        self.0.lock().unwrap().placement(kind)
    }
    fn place(&mut self, kind: WindowKind, placement: WindowPlacement) -> Result<(), String> {
        self.0.lock().unwrap().place(kind, placement)
    }
    fn options(&mut self, kind: FloatingBoardKind, top: bool, locked: bool) -> Result<(), String> {
        self.0.lock().unwrap().options(kind, top, locked)
    }
}

#[tokio::test]
async fn controller_debounces_native_bursts_and_flushes_the_latest_position_on_exit() {
    let directory = TempDir::new().unwrap();
    let (core, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
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
    within(service.apply(Action::Layout(false, false, false)))
        .await
        .unwrap();
    let initial_main = within(core.settings()).await.unwrap().display.main_window;
    assert_eq!(initial_main, Some(shared.lock().unwrap().windows["main"]));
    // No engine, socket or other real-time process exists in this test.
    tokio::time::pause();
    shared
        .lock()
        .unwrap()
        .windows
        .get_mut("main")
        .unwrap()
        .bounds
        .x = 100;
    for _ in 0..20 {
        service.changed(WindowKind::Main);
    }
    within(service.apply(Action::Layout(false, false, false)))
        .await
        .unwrap();
    within(tokio::time::advance(Duration::from_millis(400))).await;
    assert_eq!(
        within(core.settings()).await.unwrap().display.main_window,
        initial_main
    );
    shared
        .lock()
        .unwrap()
        .windows
        .get_mut("main")
        .unwrap()
        .bounds
        .x = 200;
    for _ in 0..20 {
        service.changed(WindowKind::Main);
    }
    within(service.apply(Action::Layout(false, false, false)))
        .await
        .unwrap();
    within(tokio::time::advance(Duration::from_millis(499))).await;
    assert_eq!(
        within(core.settings()).await.unwrap().display.main_window,
        initial_main
    );
    within(tokio::time::advance(Duration::from_millis(1))).await;
    // Disk completion is an external event. Return to real time before waiting
    // for its publication so the safety timeout cannot be held by a busy loop.
    tokio::time::resume();
    within(async {
        loop {
            if let AppEvent::DisplaySettingsChanged(display) =
                events.recv().await.expect("core stopped")
                && display
                    .main_window
                    .is_some_and(|position| position.bounds.x == 200)
            {
                break;
            }
        }
    })
    .await;
    shared
        .lock()
        .unwrap()
        .windows
        .get_mut("main")
        .unwrap()
        .bounds
        .x = 300;
    tokio::time::pause();
    within(tokio::time::advance(Duration::from_millis(1000))).await;
    tokio::time::resume();
    assert_eq!(
        within(core.settings())
            .await
            .unwrap()
            .display
            .main_window
            .unwrap()
            .bounds
            .x,
        200
    );
    service.changed(WindowKind::Main);
    drop(service);
    within(task).await.unwrap();
    assert_eq!(
        within(core.settings())
            .await
            .unwrap()
            .display
            .main_window
            .unwrap()
            .bounds
            .x,
        300
    );
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn partial_creation_failure_rolls_back_windows_and_all_layout_flags() {
    let (mut controller, _events, _directory) =
        setup(DisplaySettings::default(), FakeWindows::default()).await;
    controller.platform.fail_create.insert("float-sub");
    let before = controller.platform.windows.clone();
    assert!(
        within(controller.action(Action::Layout(true, true, true)))
            .await
            .is_err()
    );
    assert_eq!(controller.platform.windows, before);
    assert_eq!(
        within(controller.core.settings())
            .await
            .unwrap()
            .display
            .floating_boards,
        FloatingBoardsView::default()
    );
    assert!(
        controller
            .platform
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Create(WindowKind::FloatingMain, ..)))
    );
    assert!(
        controller
            .platform
            .effects
            .contains(&Effect::Destroy(WindowKind::FloatingMain))
    );
    finish(controller).await;
}

#[tokio::test]
async fn failed_settings_save_restores_actual_unsaved_positions_and_options() {
    let original = DisplaySettings {
        floating_boards: FloatingBoardsView {
            main: true,
            sub: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (mut controller, _events, directory) =
        setup(original.clone(), FakeWindows::default()).await;
    let baseline = within(controller.core.settings()).await.unwrap().display;
    controller
        .platform
        .windows
        .get_mut("main")
        .unwrap()
        .bounds
        .x += 25;
    let actual = controller.platform.windows.clone();
    assert!(baseline.main_window.is_some());
    assert_ne!(baseline.main_window, Some(actual["main"]));
    // A directory at the settings destination makes atomic replacement fail on
    // every platform without changing real user files or relying on Unix modes.
    std::fs::rename(
        directory.path().join("settings.json"),
        directory.path().join("original-settings.json"),
    )
    .unwrap();
    std::fs::create_dir(directory.path().join("settings.json")).unwrap();
    let requested = DisplaySettings {
        main_window: Some(WindowPlacement {
            bounds: WindowBounds {
                x: 20,
                y: 30,
                width: 700,
                height: 500,
            },
            maximized: true,
        }),
        floating_main: Some(WindowBounds {
            x: 10,
            y: 20,
            width: 300,
            height: 400,
        }),
        floating_sub: Some(WindowBounds {
            x: 900,
            y: 500,
            width: 500,
            height: 500,
        }),
        floating_boards: FloatingBoardsView {
            main: true,
            sub: true,
            main_on_top: true,
            sub_locked: true,
            ..Default::default()
        },
        ..original.clone()
    };
    assert!(
        within(controller.action(Action::ApplyLayout(WindowLayout::from(&requested))))
            .await
            .is_err()
    );
    assert_eq!(
        within(controller.core.settings()).await.unwrap().display,
        baseline
    );
    assert_eq!(controller.platform.windows, actual);
    assert_eq!(controller.platform.options["float-main"], (false, false));
    assert_eq!(controller.platform.options["float-sub"], (false, false));
    finish(controller).await;
}

#[tokio::test]
async fn fit_at_screen_edges_includes_window_decorations_in_the_minimum() {
    let mut platform = FakeWindows {
        // The native minimum is the 520x400 client plus 16x39 decorations.
        frames: [("main", (16, 39))].into(),
        ..Default::default()
    };
    let placement = platform.windows.get_mut("main").unwrap();
    placement.bounds.x = 1750;
    placement.bounds.y = 1000;
    let (mut controller, _events, _directory) = setup(DisplaySettings::default(), platform).await;
    let fitted = within(controller.action(Action::Fit(300, 200)))
        .await
        .unwrap();
    let expected = WindowPlacement {
        bounds: WindowBounds {
            x: 1384,
            y: 641,
            width: 536,
            height: 439,
        },
        maximized: false,
    };
    assert_eq!(fitted.main_window, Some(expected));
    assert_eq!(controller.platform.windows["main"], expected);
    assert_eq!(expected.bounds.x + expected.bounds.width as i32, 1920);
    assert_eq!(expected.bounds.y + expected.bounds.height as i32, 1080);
    finish(controller).await;
}

#[tokio::test]
async fn fit_uses_client_size_with_decorations_and_repeated_requests_do_not_shrink() {
    let platform = FakeWindows {
        frames: [("main", (16, 39))].into(),
        ..Default::default()
    };
    let (mut controller, _events, _directory) = setup(DisplaySettings::default(), platform).await;
    let original = controller.platform.windows["main"];
    let expected = WindowPlacement {
        bounds: WindowBounds {
            x: original.bounds.x,
            y: original.bounds.y,
            width: 816,
            height: 639,
        },
        maximized: false,
    };
    for _ in 0..3 {
        let fitted = within(controller.action(Action::Fit(800, 600)))
            .await
            .unwrap();
        let outer = controller.platform.windows["main"];
        assert_eq!(outer, expected);
        assert_eq!(fitted.main_window, Some(expected));
        let frame = controller.platform.frame(WindowKind::Main);
        assert_eq!(
            (outer.bounds.width - frame.0, outer.bounds.height - frame.1),
            (800, 600)
        );
        assert_eq!(
            within(controller.core.settings())
                .await
                .unwrap()
                .display
                .main_window,
            Some(expected)
        );
    }
    controller
        .platform
        .windows
        .get_mut("main")
        .unwrap()
        .maximized = true;
    let maximized = controller.platform.windows["main"];
    let effect_count = controller.platform.effects.len();
    within(controller.action(Action::Fit(1000, 700)))
        .await
        .unwrap();
    assert_eq!(controller.platform.windows["main"], maximized);
    assert_eq!(controller.platform.effects.len(), effect_count);
    finish(controller).await;
}
