#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use app_core::{
    AiPlayer, AppConfig, AppCore, AppError, AppEvent, AppSubscription, BatchOptions, BatchOutput,
    BatchPhase, DocumentView, EngineProfile, EngineStatus, GameSetup, GameStart, KataGoPlay,
    MatchPlayer, MatchSetup, NewGameOptions, Player, SaveSlot, Settings,
};
use common::within;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

const SOURCE: &[u8] = b"(;SZ[9];B[aa](;W[bb];B[cc])(;W[dd]C[saved side]))";

async fn start(directory: &Path) -> (AppCore, AppSubscription) {
    within(app_core::spawn(directory)).await.unwrap()
}

async fn doc(events: &mut AppSubscription, current: u32) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = events.recv().await.unwrap()
                && view.tree.current == current
            {
                return view;
            }
        }
    })
    .await
}

async fn auto_slot(app: &AppCore) -> SaveSlot {
    within(async {
        loop {
            if let Some(slot) = app
                .save_slots()
                .await
                .unwrap()
                .into_iter()
                .find(|slot| slot.is_autosave)
            {
                return slot;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}

fn metadata(directory: &Path) -> Vec<u8> {
    std::fs::read(directory.join("saves/autosave/metadata.json")).unwrap()
}

fn autosaved_sgf(directory: &Path) -> String {
    let value: Value = serde_json::from_slice(&metadata(directory)).unwrap();
    std::fs::read_to_string(
        directory
            .join("saves/autosave")
            .join(value["sgf_file"].as_str().unwrap()),
    )
    .unwrap()
}

fn autosave_files(directory: &Path) -> (Vec<u8>, Vec<u8>) {
    let metadata = metadata(directory);
    let value: Value = serde_json::from_slice(&metadata).unwrap();
    let sgf = std::fs::read(
        directory
            .join("saves/autosave")
            .join(value["sgf_file"].as_str().unwrap()),
    )
    .unwrap();
    (metadata, sgf)
}

async fn seed_autosave(directory: &Path) {
    let (app, _) = start(directory).await;
    within(app.open_bytes(SOURCE.to_vec())).await.unwrap();
    within(app.go_to(4)).await.unwrap();
    within(app.shutdown()).await.unwrap();
}

async fn advance_tick(app: &AppCore) {
    for _ in 0..60 {
        within(tokio::time::advance(Duration::from_secs(1))).await;
    }
    // Advancing schedules the timer; allow its actor to run before assertions.
    // Queries are serial barriers while filesystem work can complete off-thread.
    within(async {
        for _ in 0..8 {
            tokio::task::yield_now().await;
            app.settings().await.unwrap();
        }
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn timer_writes_only_changed_content_and_exit_records_latest_focus() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _) = start(directory.path()).await;
    let settings = within(app.settings()).await.unwrap();
    assert!(settings.autosave);
    assert!(!settings.restore_last_game);
    advance_tick(&app).await;
    assert!(within(app.save_slots()).await.unwrap().is_empty());

    within(app.open_bytes(SOURCE.to_vec())).await.unwrap();
    within(app.go_to(4)).await.unwrap();
    advance_tick(&app).await;
    let first = auto_slot(&app).await;
    assert_eq!(first.node_path, [0, 1]);
    assert_eq!(first.move_number, 2);
    let original = metadata(directory.path());
    within(app.go_to(1)).await.unwrap();
    for _ in 0..3 {
        advance_tick(&app).await;
        assert_eq!(
            metadata(directory.path()),
            original,
            "navigation must not trigger a new generation"
        );
    }

    within(app.set_comment("edited".into())).await.unwrap();
    advance_tick(&app).await;
    within(async {
        loop {
            let slots = app.save_slots().await.unwrap();
            if slots
                .iter()
                .any(|slot| slot.is_autosave && slot.node_path == [0])
                && metadata(directory.path()) != original
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(autosaved_sgf(directory.path()).contains("C[edited]"));
    let after_edit = metadata(directory.path());
    advance_tick(&app).await;
    assert_eq!(metadata(directory.path()), after_edit);
    within(app.go_to(3)).await.unwrap();
    within(app.shutdown()).await.unwrap();
    let saved: Value = serde_json::from_slice(&metadata(directory.path())).unwrap();
    assert_eq!(saved["slot"]["node_path"], serde_json::json!([0, 0, 0]));
    assert_eq!(saved["slot"]["move_number"], 3);
}

#[tokio::test(start_paused = true)]
async fn empty_startup_exit_preserves_previous_autosave_files() {
    let directory = tempfile::tempdir().unwrap();
    seed_autosave(directory.path()).await;
    let original = autosave_files(directory.path());

    let (app, mut events) = start(directory.path()).await;
    assert!(!within(app.settings()).await.unwrap().restore_last_game);
    assert_eq!(doc(&mut events, 0).await.board.unwrap().move_number, 0);
    within(app.shutdown()).await.unwrap();

    let saved = autosave_files(directory.path());
    assert!(
        saved.0 == original.0,
        "autosave metadata must stay unchanged"
    );
    assert!(saved.1 == original.1, "autosave SGF must stay unchanged");
}

#[tokio::test(start_paused = true)]
async fn new_blank_game_exit_preserves_previous_autosave_files() {
    let directory = tempfile::tempdir().unwrap();
    seed_autosave(directory.path()).await;
    let original = autosave_files(directory.path());

    let source = directory.path().join("opened.sgf");
    std::fs::write(&source, SOURCE).unwrap();
    let (app, _) = start(directory.path()).await;
    within(app.open_path(source)).await.unwrap();
    within(app.new_game(NewGameOptions::default()))
        .await
        .unwrap();
    within(app.shutdown()).await.unwrap();

    let saved = autosave_files(directory.path());
    assert!(
        saved.0 == original.0,
        "autosave metadata must stay unchanged"
    );
    assert!(saved.1 == original.1, "autosave SGF must stay unchanged");
}

#[tokio::test(start_paused = true)]
async fn exit_after_navigation_updates_focus_without_changing_sgf() {
    let directory = tempfile::tempdir().unwrap();
    seed_autosave(directory.path()).await;
    let (_, original_sgf) = autosave_files(directory.path());
    let source = directory.path().join("opened.sgf");
    std::fs::write(&source, &original_sgf).unwrap();

    let (app, _) = start(directory.path()).await;
    within(app.open_path(source)).await.unwrap();
    within(app.go_to(3)).await.unwrap();
    within(app.shutdown()).await.unwrap();

    let (metadata, saved_sgf) = autosave_files(directory.path());
    let saved: Value = serde_json::from_slice(&metadata).unwrap();
    assert_eq!(saved["slot"]["node_path"], serde_json::json!([0, 0, 0]));
    assert_eq!(saved["slot"]["move_number"], 3);
    assert_eq!(saved_sgf, original_sgf);
}

#[tokio::test(start_paused = true)]
async fn exit_saves_zero_move_games_with_comments_setup_or_game_info() {
    for property in ["C[comment]", "AB[aa]", "PB[Black]PW[White]"] {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = start(directory.path()).await;
        within(app.open_bytes(format!("(;{property})").into_bytes()))
            .await
            .unwrap();
        within(app.shutdown()).await.unwrap();

        let saved: Value = serde_json::from_slice(&metadata(directory.path())).unwrap();
        assert_eq!(saved["slot"]["move_number"], 0);
        assert!(autosaved_sgf(directory.path()).contains(property));
    }
}

#[tokio::test(start_paused = true)]
async fn disabled_autosave_and_failed_settings_writes_preserve_preferences() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _) = start(directory.path()).await;
    assert!(!within(app.set_autosave(false)).await.unwrap());
    assert!(within(app.set_restore_last_game(true)).await.unwrap());
    within(app.open_bytes(SOURCE.to_vec())).await.unwrap();
    advance_tick(&app).await;
    assert!(within(app.save_slots()).await.unwrap().is_empty());
    let saved = within(Settings::load(directory.path())).await.unwrap();
    assert!(!saved.autosave);
    assert!(saved.restore_last_game);

    let settings_path = directory.path().join("settings.json");
    std::fs::rename(&settings_path, directory.path().join("saved-settings.json")).unwrap();
    std::fs::create_dir(&settings_path).unwrap();
    assert!(within(app.set_autosave(true)).await.is_err());
    assert!(within(app.set_restore_last_game(false)).await.is_err());
    let retained = within(app.settings()).await.unwrap();
    assert!(!retained.autosave);
    assert!(retained.restore_last_game);
    within(app.shutdown()).await.unwrap();
    assert!(!directory.path().join("saves").exists());
}

#[tokio::test(start_paused = true)]
async fn shutdown_and_last_handle_drop_save_once_and_write_failures_allow_shutdown() {
    for drop_handle in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (app, mut events) = start(directory.path()).await;
        within(app.open_bytes(SOURCE.to_vec())).await.unwrap();
        within(app.go_to(4)).await.unwrap();
        if drop_handle {
            drop(app);
        } else {
            within(app.shutdown()).await.unwrap();
        }
        within(async { while events.recv().await.is_some() {} }).await;
        let value: Value = serde_json::from_slice(&metadata(directory.path())).unwrap();
        assert_eq!(value["slot"]["node_path"], serde_json::json!([0, 1]));
        let files: Vec<_> = std::fs::read_dir(directory.path().join("saves/autosave"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(
            files.len(),
            2,
            "one metadata file and one SGF survive shutdown"
        );
    }
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("saves"), b"not a directory").unwrap();
    let (app, mut events) = start(directory.path()).await;
    within(app.set_comment("recoverable".into())).await.unwrap();
    advance_tick(&app).await;
    within(app.shutdown()).await.unwrap();
    within(async {
        while let Some(event) = events.recv().await {
            assert!(!matches!(event, AppEvent::OpenFailed { .. }));
        }
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn restore_switch_controls_startup_and_failed_restore_is_quiet() {
    for restore in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = start(directory.path()).await;
        within(app.set_restore_last_game(restore)).await.unwrap();
        within(app.open_bytes(SOURCE.to_vec())).await.unwrap();
        within(app.go_to(4)).await.unwrap();
        within(app.shutdown()).await.unwrap();
        let (app, mut events) = start(directory.path()).await;
        let restored = doc(&mut events, if restore { 4 } else { 0 }).await;
        if restore {
            assert_eq!(restored.comment.as_deref(), Some("saved side"));
            assert_eq!(restored.board.unwrap().move_number, 2);
            assert_eq!(restored.file_path, None);
            assert!(!restored.modified);
        } else {
            assert_eq!(restored.board.unwrap().move_number, 0);
            assert_eq!(within(app.variation_tree()).await.unwrap().nodes.len(), 1);
        }
        within(app.set_autosave(false)).await.unwrap();
        within(app.shutdown()).await.unwrap();
    }

    for broken in ["missing", "metadata", "sgf"] {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = start(directory.path()).await;
        within(app.set_restore_last_game(true)).await.unwrap();
        within(app.open_bytes(SOURCE.to_vec())).await.unwrap();
        if broken == "missing" {
            within(app.set_autosave(false)).await.unwrap();
        }
        within(app.shutdown()).await.unwrap();
        if broken == "metadata" {
            std::fs::write(
                directory.path().join("saves/autosave/metadata.json"),
                b"broken",
            )
            .unwrap();
        } else if broken == "sgf" {
            let value: Value = serde_json::from_slice(&metadata(directory.path())).unwrap();
            std::fs::write(
                directory
                    .path()
                    .join("saves/autosave")
                    .join(value["sgf_file"].as_str().unwrap()),
                b"broken",
            )
            .unwrap();
        }
        let (app, mut events) = start(directory.path()).await;
        within(app.settings()).await.unwrap();
        assert_eq!(within(app.variation_tree()).await.unwrap().nodes.len(), 1);
        within(app.set_autosave(false)).await.unwrap();
        within(app.shutdown()).await.unwrap();
        within(async {
            while let Some(event) = events.recv().await {
                assert!(
                    !matches!(event, AppEvent::OpenFailed { .. }),
                    "optional restoration must only log failures"
                );
            }
        })
        .await;
    }
}

#[tokio::test(start_paused = true)]
async fn delete_all_preserves_autosave_and_data_directory_is_separate_from_settings() {
    let settings_directory = tempfile::tempdir().unwrap();
    let data_directory = tempfile::tempdir().unwrap();
    let (app, _) = within(app_core::spawn_with_config(
        settings_directory.path(),
        AppConfig {
            cache_directory: Some(data_directory.path().to_owned()),
            ..AppConfig::default()
        },
    ))
    .await
    .unwrap();
    within(app.open_bytes(SOURCE.to_vec())).await.unwrap();
    let manual = within(app.save_to_new_slot("manual".into())).await.unwrap();
    advance_tick(&app).await;
    let automatic = auto_slot(&app).await;
    assert!(
        data_directory
            .path()
            .join("saves")
            .join(&manual.id)
            .exists()
    );
    assert!(!settings_directory.path().join("saves").exists());
    within(app.delete_all_slots()).await.unwrap();
    assert_eq!(within(app.save_slots()).await.unwrap(), vec![automatic]);
    within(app.shutdown()).await.unwrap();
}

fn fake_program() -> PathBuf {
    static PROGRAM: OnceLock<PathBuf> = OnceLock::new();
    PROGRAM
        .get_or_init(|| fake_katago::binary_path().unwrap())
        .clone()
}

async fn real_time_engine(directory: &Path) -> (AppCore, AppSubscription) {
    let program = fake_program();
    let model = directory.join("fake-model");
    let config_path = directory.join("fake-config");
    let script = directory.join("script.json");
    std::fs::write(&model, b"model").unwrap();
    std::fs::write(&config_path, b"").unwrap();
    std::fs::write(&script, br#"{"default":{"hang":true}}"#).unwrap();
    within(
        Settings {
            engine: Some(EngineProfile {
                program,
                model,
                config: config_path,
                extra_args: vec![
                    "--play-script".into(),
                    script.to_string_lossy().into_owned(),
                ],
                ..EngineProfile::default()
            }),
            pondering: false,
            cache_enabled: false,
            auto_quick_analyze_on_load: false,
            autosave: false,
            ..Settings::default()
        }
        .save(directory),
    )
    .await
    .unwrap();
    let mut config = AppConfig {
        autosave_interval: Duration::from_millis(20),
        ..AppConfig::default()
    };
    config.session.response_timeout = Duration::from_secs(60);
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(2);
    let (app, mut events) = within(app_core::spawn_with_config(directory, config))
        .await
        .unwrap();
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::EngineStatusChanged(EngineStatus::Ready { .. }) => break,
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("{reason}")
                }
                _ => {}
            }
        }
    })
    .await;
    (app, events)
}

async fn assert_no_autosave_for_ticks(app: &AppCore) {
    // Observe several production timer periods with real time while a child is
    // alive; pausing Tokio time here would hang Windows subprocess pipe reads.
    within(async {
        let end = tokio::time::Instant::now() + Duration::from_millis(100);
        while tokio::time::Instant::now() < end {
            assert!(
                app.save_slots()
                    .await
                    .unwrap()
                    .iter()
                    .all(|slot| !slot.is_autosave)
            );
            tokio::task::yield_now().await;
        }
    })
    .await;
}

#[tokio::test]
async fn games_matches_and_batch_suppress_timer_and_reuse_open_guards() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _) = real_time_engine(directory.path()).await;
    let slot = within(app.save_to_new_slot("before game".into()))
        .await
        .unwrap();
    let setup = GameSetup {
        white: Player::Ai(AiPlayer::KataGo(KataGoPlay {
            seconds: None,
            visits: Some(10),
            resign: None,
            ..KataGoPlay::default()
        })),
        ponder: false,
        auto_save: false,
        ..GameSetup::default()
    };
    within(app.start_game(setup)).await.unwrap();
    within(app.play(go_core::Move::Play(
        go_core::Color::Black,
        go_core::Point { x: 0, y: 0 },
    )))
    .await
    .unwrap();
    within(app.set_autosave(true)).await.unwrap();
    assert!(matches!(
        within(app.load_slot(slot.id.clone())).await,
        Err(AppError::GameRunning)
    ));
    assert_no_autosave_for_ticks(&app).await;
    within(app.pause_game()).await.unwrap();
    assert_no_autosave_for_ticks(&app).await;
    within(app.set_autosave(false)).await.unwrap();
    within(app.stop_game()).await.unwrap();

    let id = within(app.settings())
        .await
        .unwrap()
        .main_engine_id
        .unwrap();
    let player = MatchPlayer::KataGo {
        engine_id: id,
        play: KataGoPlay {
            seconds: None,
            visits: Some(10),
            resign: None,
            ..Default::default()
        },
        wrn: 0.0,
        pda: 0.0,
    };
    within(app.start_match(MatchSetup {
        a: player.clone(),
        b: player,
        start: GameStart::CurrentPosition,
        auto_save: false,
        ..MatchSetup::default()
    }))
    .await
    .unwrap();
    within(app.set_autosave(true)).await.unwrap();
    assert!(within(app.load_slot(slot.id.clone())).await.is_err());
    assert_no_autosave_for_ticks(&app).await;
    within(app.set_autosave(false)).await.unwrap();
    within(app.stop_match()).await.unwrap();

    let source = directory.path().join("batch.sgf");
    std::fs::write(&source, SOURCE).unwrap();
    within(app.start_batch_analysis(BatchOptions {
        files: vec![source],
        visits: 100_000,
        scope: app_core::WholeGameScope::Line,
        force: false,
        output: BatchOutput {
            suffix: "review".into(),
            directory: None,
        },
    }))
    .await
    .unwrap();
    within(async {
        loop {
            let status = app.batch_analysis_status().await.unwrap();
            if status.phase == BatchPhase::Running && status.current_index.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    within(app.set_autosave(true)).await.unwrap();
    assert!(matches!(
        within(app.load_slot(slot.id.clone())).await,
        Err(AppError::BatchAnalysisRunning)
    ));
    assert_no_autosave_for_ticks(&app).await;
    within(app.pause_batch_analysis()).await.unwrap();
    assert_no_autosave_for_ticks(&app).await;
    within(app.set_autosave(false)).await.unwrap();
    within(app.stop_batch_analysis()).await.unwrap();
    within(app.set_autosave(true)).await.unwrap();
    auto_slot(&app).await;
    within(app.shutdown()).await.unwrap();
}
