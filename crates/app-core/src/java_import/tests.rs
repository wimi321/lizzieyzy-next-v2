use super::*;
use crate::{DisplaySettings, EngineProfile, EngineRules, EngineSettings};
use proptest::prelude::*;
use serde_json::json;

const CONFIG: &[u8] = include_bytes!("../../../../fixtures/java-config/config.txt");
const PERSIST: &[u8] = include_bytes!("../../../../fixtures/java-config/persist");

async fn source() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    tokio::fs::write(directory.path().join("config.txt"), CONFIG)
        .await
        .unwrap();
    tokio::fs::write(directory.path().join("persist"), PERSIST)
        .await
        .unwrap();
    directory
}

#[tokio::test]
async fn golden_preview_and_confirm_only_write_new_settings_and_do_not_start_imported_engines() {
    let source = source().await;
    let destination = tempfile::tempdir().unwrap();
    let (app, _events) = crate::spawn(destination.path()).await.unwrap();
    let before = app.settings().await.unwrap();
    let preview = app
        .preview_java_import(source.path().to_owned())
        .await
        .unwrap();
    assert_eq!(app.settings().await.unwrap(), before);
    assert!(!destination.path().join("settings.json").exists());
    assert_eq!(preview.engines.len(), 2);
    assert_eq!(preview.settings.engines.len(), 2);
    assert_eq!(
        preview.imported_main_engine_id,
        Some(preview.engines[1].entry.id.clone())
    );
    assert_eq!(
        preview.settings.main_engine_id,
        preview.imported_main_engine_id
    );
    assert_eq!(preview.settings.engine, None);
    assert!(
        preview
            .engines
            .iter()
            .all(|engine| engine.issues.iter().any(|issue| issue.contains("missing")))
    );
    assert!(
        preview.engines[1]
            .issues
            .iter()
            .any(|issue| issue.contains("GTP"))
    );
    assert_eq!(
        preview.engines[0].entry.profile.program,
        preview.directory.join("engines/katago engine")
    );
    assert_eq!(
        preview.engines[0].entry.profile.model,
        preview.directory.join("weights/model one.bin.gz")
    );
    assert_eq!(
        preview.engines[0].entry.profile.extra_args,
        ["-override-config", "nnCacheSizePowerOfTwo=20"]
    );
    assert_eq!(
        preview.settings.engine_settings,
        EngineSettings {
            rules: EngineRules::Japanese,
            read_komi: false,
            default_komi: 6.5,
            auto_check_alive: false,
            report_interval_ms: 250,
        }
    );
    assert_eq!(
        preview.settings.display,
        DisplaySettings {
            show_coordinates: false,
            move_numbers: crate::MoveNumberMode::LastN,
            last_move_numbers: 17,
            move_numbers_from_one: true,
            branch_move_numbers_from_one: false,
            show_all_branch_move_numbers: true,
            move_rank_marks: crate::MoveRankMarkMode::LastN,
            last_move_rank_marks: 8,
            show_winrate_in_suggestion: false,
            show_visits_in_suggestion: false,
            show_score_in_suggestion: false,
            show_score_as_diff: true,
            show_suggestion_order: false,
            winrate_always_black: true,
            show_hover_variation: false,
            max_suggestions: 7,
            suggestion_max_red: true,
            next_moves: crate::NextMoveHint::Simple,
            next_move_min_visits: 123,
            show_winrate_graph: false,
            show_comment: false,
            show_variation_graph: false,
            show_sub_board: false,
            show_list_pane: false,
            show_information_panel: false,
            show_status_bar: false,
            show_top_toolbar: false,
            show_bottom_toolbar: false,
            winrate_graph_mode: crate::WinrateGraphMode::ScoreLead,
            fill_winrate_graph: false,
            show_blunder_bar: true,
            score_lead_includes_komi: false,
            confirm_new_board: false,
            confirm_overwrite: false,
            show_names_below_board: false,
        }
    );
    let saved = app
        .confirm_java_import(preview.preview_id.clone())
        .await
        .unwrap();
    assert_eq!(saved, preview.settings);
    assert_eq!(Settings::load(destination.path()).await.unwrap(), saved);
    assert!(app.confirm_java_import(preview.preview_id).await.is_err());
    // Reimport is idempotent, preserving stable IDs instead of appending copies.
    let again = app
        .preview_java_import(source.path().to_owned())
        .await
        .unwrap();
    assert_eq!(again.settings.engines.len(), 2);
    assert_eq!(
        tokio::fs::read(source.path().join("config.txt"))
            .await
            .unwrap(),
        CONFIG
    );
    assert_eq!(
        tokio::fs::read(source.path().join("persist"))
            .await
            .unwrap(),
        PERSIST
    );
    assert_eq!(std::fs::read_dir(source.path()).unwrap().count(), 2);
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn missing_java_fields_preserve_existing_preferences_and_legacy_list_order() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    tokio::fs::write(
        source.path().join("config.txt"),
        json!({
            "leelaz": {
                "engine-command": "katago analysis -model first.bin -config first.cfg",
                "engine-command-list": ["katago analysis -model second.bin -config second.cfg"],
                "engine-name-list": ["First", "Second"]
            },
            "ui": {"autoload-last": true, "last-engine": 1}
        })
        .to_string(),
    )
    .await
    .unwrap();
    let base = Settings {
        pondering: false,
        display: DisplaySettings {
            show_coordinates: false,
            ..Default::default()
        },
        ..Default::default()
    };
    base.save(destination.path()).await.unwrap();
    let (app, _events) = crate::spawn(destination.path()).await.unwrap();
    let preview = app
        .preview_java_import(source.path().to_owned())
        .await
        .unwrap();
    assert_eq!(
        preview
            .engines
            .iter()
            .map(|entry| entry.entry.name.as_str())
            .collect::<Vec<_>>(),
        ["First", "Second"]
    );
    assert_eq!(
        preview.imported_main_engine_id,
        Some(preview.engines[1].entry.id.clone())
    );
    assert_eq!(preview.settings.display, base.display);
    assert_eq!(preview.settings.engine_settings, base.engine_settings);
    assert!(!preview.settings.pondering);
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn stale_or_replaced_preview_cannot_overwrite_new_settings() {
    let source = source().await;
    let destination = tempfile::tempdir().unwrap();
    let (app, _events) = crate::spawn(destination.path()).await.unwrap();
    let old = app
        .preview_java_import(source.path().to_owned())
        .await
        .unwrap();
    let latest = app
        .preview_java_import(source.path().to_owned())
        .await
        .unwrap();
    assert!(app.confirm_java_import(old.preview_id).await.is_err());
    app.set_display_settings(DisplaySettings {
        show_coordinates: false,
        ..Default::default()
    })
    .await
    .unwrap();
    let current = app.settings().await.unwrap();
    assert!(app.confirm_java_import(latest.preview_id).await.is_err());
    assert_eq!(app.settings().await.unwrap(), current);
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn confirmation_uses_reviewed_bytes_and_save_failure_is_retryable() {
    let source = source().await;
    let destination = tempfile::tempdir().unwrap();
    let (app, _events) = crate::spawn(destination.path()).await.unwrap();
    let preview = app
        .preview_java_import(source.path().to_owned())
        .await
        .unwrap();
    tokio::fs::write(source.path().join("config.txt"), b"{}")
        .await
        .unwrap();
    tokio::fs::create_dir(destination.path().join("settings.json"))
        .await
        .unwrap();
    assert!(
        app.confirm_java_import(preview.preview_id.clone())
            .await
            .is_err()
    );
    assert!(app.settings().await.unwrap().engines.is_empty());
    tokio::fs::remove_dir(destination.path().join("settings.json"))
        .await
        .unwrap();
    assert_eq!(
        app.confirm_java_import(preview.preview_id).await.unwrap(),
        preview.settings
    );
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn import_keeps_existing_main_engine_and_unrelated_settings() {
    let source = source().await;
    let destination = tempfile::tempdir().unwrap();
    let profile = EngineProfile::default();
    let base = Settings {
        engines: vec![EngineEntry {
            id: "existing".into(),
            name: "Existing".into(),
            profile,
        }],
        main_engine_id: Some("existing".into()),
        recent_files: vec!["prior.sgf".into()],
        ..Default::default()
    };
    base.save(destination.path()).await.unwrap();
    let (app, _events) = crate::spawn(destination.path()).await.unwrap();
    let preview = app
        .preview_java_import(source.path().to_owned())
        .await
        .unwrap();
    assert_eq!(preview.settings.main_engine_id, base.main_engine_id);
    assert_eq!(preview.settings.recent_files, base.recent_files);
    assert_eq!(preview.settings.engines[0], base.engines[0]);
    assert!(
        preview
            .issues
            .iter()
            .any(|issue| issue.contains("existing main engine was retained"))
    );
    app.confirm_java_import(preview.preview_id).await.unwrap();
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn source_directory_and_its_descendants_are_never_settings_destinations() {
    let source = source().await;
    for destination in [source.path().to_owned(), source.path().join("new/subdir")] {
        assert!(
            ensure_separate_directory(source.path(), &destination)
                .await
                .is_err()
        );
    }
    #[cfg(unix)]
    {
        let alias = tempfile::tempdir().unwrap();
        let path = alias.path().join("java-link");
        std::os::unix::fs::symlink(source.path(), &path).unwrap();
        assert!(
            ensure_separate_directory(source.path(), &path)
                .await
                .is_err()
        );
        let destination = tempfile::tempdir().unwrap();
        Settings::default().save(destination.path()).await.unwrap();
        let linked_source = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(
            destination.path().join("settings.json"),
            linked_source.path().join("config.txt"),
        )
        .unwrap();
        assert!(
            ensure_separate_directory(linked_source.path(), destination.path())
                .await
                .is_err()
        );
    }
    assert_eq!(
        tokio::fs::read(source.path().join("config.txt"))
            .await
            .unwrap(),
        CONFIG
    );
}

#[tokio::test]
async fn remote_credentials_are_not_imported_and_bad_input_does_not_write() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    tokio::fs::write(source.path().join("config.txt"), json!({"leelaz": {
        "engine-settings-list": [{"name":"Remote","command":"ssh server remote-secret", "useJavaSSH":true,"password":"test-secret","keyGenPath":"private-key"}]
    }}).to_string()).await.unwrap();
    let (app, _events) = crate::spawn(destination.path()).await.unwrap();
    let preview = app
        .preview_java_import(source.path().to_owned())
        .await
        .unwrap();
    assert!(
        preview.engines[0]
            .issues
            .iter()
            .any(|issue| issue.contains("remote SSH"))
    );
    let serialized = serde_json::to_string(&preview).unwrap();
    assert!(!serialized.contains("test-secret"));
    assert!(!serialized.contains("private-key"));
    assert!(!serialized.contains("remote-secret"));
    assert!(
        preview.engines[0]
            .entry
            .profile
            .program
            .as_os_str()
            .is_empty()
    );
    for content in [
        "[]",
        "{",
        r#"{"ui":{"read-komi":"false"}}"#,
        r#"{"leelaz":{"engine-command":"'unterminated"}}"#,
    ] {
        tokio::fs::write(source.path().join("config.txt"), content)
            .await
            .unwrap();
        assert!(
            app.preview_java_import(source.path().to_owned())
                .await
                .is_err()
        );
    }
    assert!(!destination.path().join("settings.json").exists());
    app.shutdown().await.unwrap();
}

#[test]
fn display_modes_and_disabled_java_rules_preserve_their_meaning() {
    let mut display = DisplaySettings::default();
    let mut issues = Vec::new();
    for (count, mode) in [
        (0, crate::MoveNumberMode::None),
        (-1, crate::MoveNumberMode::All),
        (1, crate::MoveNumberMode::LastOne),
        (5, crate::MoveNumberMode::LastFive),
        (10, crate::MoveNumberMode::LastTen),
    ] {
        display::merge(
            &json!({"allow-move-number":count}),
            &Value::Null,
            &Value::Null,
            &mut display,
            &mut issues,
        )
        .unwrap();
        assert_eq!(display.move_numbers, mode);
    }
    display::merge(&json!({"move-rank-mark-last-move":0,"show-next-moves":false,"show-win-rate-line":false,"show-score-lead-line":false}), &json!({"limit-max-suggestion":-1}), &Value::Null, &mut display, &mut issues).unwrap();
    assert_eq!(display.move_rank_marks, crate::MoveRankMarkMode::All);
    assert_eq!(display.next_moves, crate::NextMoveHint::None);
    assert_eq!(display.max_suggestions, 0);
    assert_eq!(display.winrate_graph_mode, crate::WinrateGraphMode::Both);
    assert!(!issues.is_empty());
    let mut settings = EngineSettings {
        rules: EngineRules::Korean,
        ..Default::default()
    };
    options::merge(
        &json!({"auto-load-kata-rules":false,"kata-rules":"japanese"}),
        &Value::Null,
        &mut settings,
        &mut issues,
    )
    .unwrap();
    assert_eq!(settings.rules, EngineRules::FollowSgf);
}

proptest! {
    #[test]
    fn java_quoted_token_round_trip_preserves_spaces_and_windows_paths(tokens in prop::collection::vec("[a-zA-Z0-9 /\\\\_.:-]{0,40}", 0..20)) {
        let command = tokens.iter().map(|token| format!("\"{token}\"")).collect::<Vec<_>>().join(" ");
        prop_assert_eq!(engines::split_command(&command).unwrap(), tokens);
    }
}
