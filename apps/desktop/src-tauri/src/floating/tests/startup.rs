use super::*;

#[tokio::test]
async fn empty_settings_capture_actual_main_placement_and_maximize_before_any_move() {
    for maximized in [false, true] {
        let directory = TempDir::new().unwrap();
        let (core, _events) = within(app_core::spawn(directory.path())).await.unwrap();
        assert_eq!(
            within(core.settings()).await.unwrap().display.main_window,
            None
        );
        let mut platform = FakeWindows::default();
        let actual = WindowPlacement {
            bounds: WindowBounds {
                x: -900,
                y: 45,
                width: 830,
                height: 620,
            },
            maximized,
        };
        platform.windows.insert("main", actual);
        let mut controller = Controller {
            core,
            platform,
            placements: Placements::default(),
            reset_bounds: BTreeMap::new(),
        };
        within(controller.initialize()).await;
        assert_eq!(
            within(controller.core.settings())
                .await
                .unwrap()
                .display
                .main_window,
            Some(actual)
        );
        assert_eq!(
            within(app_core::Settings::load(directory.path()))
                .await
                .unwrap()
                .display
                .main_window,
            Some(actual)
        );
        assert_eq!(controller.platform.windows["main"], actual);
        assert!(
            controller.platform.effects.is_empty(),
            "capturing the initial native position must not move the window"
        );
        finish(controller).await;
    }
}

#[tokio::test]
async fn existing_main_placement_is_restored_instead_of_overwritten_by_startup_position() {
    let saved = WindowPlacement {
        bounds: WindowBounds {
            x: 70,
            y: 80,
            width: 850,
            height: 630,
        },
        maximized: true,
    };
    let platform = FakeWindows::default();
    assert_ne!(platform.windows["main"], saved);
    let (controller, _events, directory) = setup(
        DisplaySettings {
            main_window: Some(saved),
            ..Default::default()
        },
        platform,
    )
    .await;
    assert_eq!(controller.platform.windows["main"], saved);
    assert_eq!(
        within(controller.core.settings())
            .await
            .unwrap()
            .display
            .main_window,
        Some(saved)
    );
    assert_eq!(
        within(app_core::Settings::load(directory.path()))
            .await
            .unwrap()
            .display
            .main_window,
        Some(saved)
    );
    assert!(
        controller
            .platform
            .effects
            .contains(&Effect::Place(WindowKind::Main, saved))
    );
    finish(controller).await;
}

#[tokio::test]
async fn unavailable_initial_main_placement_keeps_it_unset_and_restores_floating_boards() {
    for missing in [false, true] {
        let mut platform = FakeWindows::default();
        if missing {
            platform.missing_placement.insert("main");
        } else {
            platform.fail_placement.insert("main");
        }
        let actual_main = platform.windows["main"];
        let floating = FloatingBoardsView {
            main: true,
            sub: true,
            main_on_top: true,
            sub_locked: true,
            ..Default::default()
        };
        let (controller, _events, directory) = setup(
            DisplaySettings {
                floating_boards: floating,
                ..Default::default()
            },
            platform,
        )
        .await;
        let settings = within(controller.core.settings()).await.unwrap().display;
        assert_eq!(settings.main_window, None);
        assert_eq!(settings.floating_boards, floating);
        assert_eq!(controller.platform.windows["main"], actual_main);
        assert!(controller.platform.exists(WindowKind::FloatingMain));
        assert!(controller.platform.exists(WindowKind::FloatingSub));
        assert_eq!(controller.platform.options["float-main"], (true, false));
        assert_eq!(controller.platform.options["float-sub"], (false, true));
        assert_eq!(
            within(app_core::Settings::load(directory.path()))
                .await
                .unwrap()
                .display
                .main_window,
            None
        );
        finish(controller).await;
    }
}
