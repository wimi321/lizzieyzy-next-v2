#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use app_core::{AppError, AppEvent, GtpConsoleDirection, GtpEngineEntry, GtpError, Settings};
use common::within;
use std::{path::PathBuf, sync::OnceLock};

fn binary() -> PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
            let output = std::process::Command::new(
                std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()),
            )
            .args(["build", "--locked", "--manifest-path"])
            .arg(root.join("tools/fake-gtp/Cargo.toml"))
            .arg("--target-dir")
            .arg(root.join("target"))
            .arg("--message-format=json")
            .output()
            .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .filter_map(|line| {
                    let value: serde_json::Value = serde_json::from_str(line).ok()?;
                    (value["reason"] == "compiler-artifact"
                        && value["target"]["name"] == "fake-gtp")
                        .then(|| value["executable"].as_str().map(PathBuf::from))
                        .flatten()
                })
                .next()
                .unwrap()
        })
        .clone()
}
fn entry() -> GtpEngineEntry {
    GtpEngineEntry {
        id: "gtp-a".into(),
        name: "Test GTP".into(),
        program: binary(),
        args: vec![],
        working_dir: None,
        init_commands: vec!["echo initialized".into()],
    }
}

#[tokio::test]
async fn engine_crud_is_persistent_separate_and_transactional() {
    let mut entry = entry();
    let temp = tempfile::tempdir().unwrap();
    let (core, _) = app_core::spawn(temp.path()).await.unwrap();
    let added = within(core.add_gtp_engine(entry.clone())).await.unwrap();
    assert_eq!(added.gtp_engines, vec![entry.clone()]);
    assert!(added.engines.is_empty());
    assert!(added.engine.is_none());
    assert!(within(core.add_gtp_engine(entry.clone())).await.is_err());
    entry.name = "Renamed".into();
    within(core.update_gtp_engine(entry.clone())).await.unwrap();
    assert_eq!(
        Settings::load(temp.path()).await.unwrap().gtp_engines,
        vec![entry.clone()]
    );
    let mut bad = entry.clone();
    bad.init_commands = vec!["name\nquit".into()];
    assert!(within(core.update_gtp_engine(bad)).await.is_err());
    assert_eq!(
        within(core.settings()).await.unwrap().gtp_engines,
        vec![entry]
    );
    within(core.remove_gtp_engine("gtp-a".into()))
        .await
        .unwrap();
    assert!(
        Settings::load(temp.path())
            .await
            .unwrap()
            .gtp_engines
            .is_empty()
    );
    within(core.clone().shutdown()).await.unwrap();
}

#[tokio::test]
async fn console_starts_reuses_and_pushes_wire_lines_test_quits() {
    let mut entry = entry();
    let temp = tempfile::tempdir().unwrap();
    let launch = temp.path().join("launch.log");
    let exited = temp.path().join("exit.log");
    entry.args = vec![
        "--launch-log".into(),
        launch.display().to_string(),
        "--exit-log".into(),
        exited.display().to_string(),
    ];
    let (core, mut events) = app_core::spawn(temp.path()).await.unwrap();
    within(core.add_gtp_engine(entry)).await.unwrap();
    assert!(
        within(core.gtp_console("gtp-a".into()))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        within(core.gtp_console_send("gtp-a".into(), "echo hello".into()))
            .await
            .unwrap(),
        "hello"
    );
    assert_eq!(
        within(core.gtp_console_send("gtp-a".into(), "name".into()))
            .await
            .unwrap(),
        "Fake GTP"
    );
    assert_eq!(std::fs::read_to_string(&launch).unwrap().lines().count(), 1);
    within(async {
        loop {
            if let AppEvent::GtpConsoleLine(line) = events.recv().await.unwrap()
                && line.direction == GtpConsoleDirection::Sent
                && line.text.contains("echo hello")
            {
                break;
            }
        }
    })
    .await;
    let logs = within(core.gtp_console("gtp-a".into())).await.unwrap();
    assert!(logs.iter().any(|line| line.text.contains("initialized")));
    assert!(
        logs.iter()
            .any(|line| line.direction == GtpConsoleDirection::Received
                && line.text.contains("hello"))
    );
    assert!(matches!(
        within(core.gtp_console_send("gtp-a".into(), "name\nquit".into())).await,
        Err(AppError::Gtp(GtpError::InvalidCommand(_)))
    ));
    let info = within(core.test_gtp_engine("gtp-a".into())).await.unwrap();
    assert_eq!(info.name, "Fake GTP");
    assert!(info.commands.contains(&"genmove".into()));
    assert_eq!(std::fs::read_to_string(&launch).unwrap().lines().count(), 2);
    assert_eq!(std::fs::read_to_string(&exited).unwrap().lines().count(), 2);
    assert!(
        !within(core.gtp_console("gtp-a".into()))
            .await
            .unwrap()
            .is_empty()
    );
    within(core.clone().shutdown()).await.unwrap();
}

#[tokio::test]
async fn active_and_paused_game_reject_state_changes_before_sending() {
    let entry = entry();
    let temp = tempfile::tempdir().unwrap();
    let (core, _) = app_core::spawn(temp.path()).await.unwrap();
    within(core.add_gtp_engine(entry)).await.unwrap();
    let setup = app_core::GameSetup {
        white: app_core::Player::Ai(app_core::AiPlayer::Gtp {
            engine_id: "gtp-a".into(),
            time: app_core::GtpTime::EngineDefault,
        }),
        auto_save: false,
        ..Default::default()
    };
    within(core.start_game(setup)).await.unwrap();
    for pause in [false, true] {
        if pause {
            within(core.pause_game()).await.unwrap();
        }
        for text in [
            "play B D4",
            "8 GENMOVE W",
            "undo",
            "clear_board",
            "boardsize 9",
            "komi 6.5",
            "fixed_handicap 2",
            "place_free_handicap 2",
            "set_free_handicap D4 Q16",
            "loadsgf a.sgf",
            "quit",
        ] {
            assert!(
                matches!(
                    within(core.gtp_console_send("gtp-a".into(), text.into())).await,
                    Err(AppError::Gtp(GtpError::StateChangingCommand { .. }))
                ),
                "{text}"
            );
        }
        assert!(matches!(
            within(core.remove_gtp_engine("gtp-a".into())).await,
            Err(AppError::Gtp(GtpError::Busy(_)))
        ));
        assert_eq!(
            within(core.gtp_console_send("gtp-a".into(), "name".into()))
                .await
                .unwrap(),
            "Fake GTP"
        );
    }
    let logs = within(core.gtp_console("gtp-a".into())).await.unwrap();
    assert!(
        !logs.iter().any(
            |line| line.direction == GtpConsoleDirection::Sent && line.text.contains("loadsgf")
        )
    );
    within(core.stop_game()).await.unwrap();
    within(core.clone().shutdown()).await.unwrap();
}

#[tokio::test]
async fn console_and_test_handshakes_do_not_block_app_shutdown() {
    for testing in [false, true] {
        let mut entry = entry();
        entry.args = vec!["--hang-command".into(), "protocol_version".into()];
        let temp = tempfile::tempdir().unwrap();
        let (core, _) = app_core::spawn(temp.path()).await.unwrap();
        within(core.add_gtp_engine(entry)).await.unwrap();
        let request = async {
            if testing {
                core.test_gtp_engine("gtp-a".into()).await.map(|_| ())
            } else {
                core.gtp_console_send("gtp-a".into(), "name".into())
                    .await
                    .map(|_| ())
            }
        };
        let mut request = Box::pin(request);
        use std::{
            future::Future,
            task::{Context, Waker},
        };
        assert!(
            request
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        within(core.settings()).await.unwrap();
        within(core.clone().shutdown()).await.unwrap();
        assert!(within(request).await.is_err());
    }
}

#[tokio::test]
async fn test_reservation_prevents_game_reuse_and_entry_changes() {
    use std::{
        future::Future,
        task::{Context, Waker},
    };
    let mut entry = entry();
    entry.args = vec!["--hang-command".into(), "name".into()];
    let temp = tempfile::tempdir().unwrap();
    let (core, mut events) = app_core::spawn(temp.path()).await.unwrap();
    within(core.add_gtp_engine(entry.clone())).await.unwrap();
    let mut testing = Box::pin(core.test_gtp_engine("gtp-a".into()));
    assert!(
        testing
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    within(async {
        loop {
            if let AppEvent::GtpConsoleLine(line) = events.recv().await.unwrap()
                && line.direction == GtpConsoleDirection::Sent
                && line.text == "name"
            {
                break;
            }
        }
    })
    .await;
    let setup = app_core::GameSetup {
        white: app_core::Player::Ai(app_core::AiPlayer::Gtp {
            engine_id: "gtp-a".into(),
            time: app_core::GtpTime::EngineDefault,
        }),
        auto_save: false,
        ..Default::default()
    };
    assert!(matches!(
        within(core.start_game(setup)).await,
        Err(AppError::Gtp(GtpError::Busy(_)))
    ));
    assert!(matches!(
        within(core.update_gtp_engine(entry)).await,
        Err(AppError::Gtp(GtpError::Busy(_)))
    ));
    assert!(matches!(
        within(core.remove_gtp_engine("gtp-a".into())).await,
        Err(AppError::Gtp(GtpError::Busy(_)))
    ));
    assert!(matches!(
        within(core.test_gtp_engine("gtp-a".into())).await,
        Err(AppError::Gtp(GtpError::Busy(_)))
    ));
    assert!(within(core.game_view()).await.unwrap().is_none());
    within(core.clone().shutdown()).await.unwrap();
    assert!(within(testing).await.is_err());
}

#[tokio::test]
async fn queued_console_reply_does_not_block_pause_during_genmove() {
    use std::{
        future::Future,
        task::{Context, Waker},
    };
    let mut entry = entry();
    entry.args = vec!["--hang-command".into(), "genmove".into()];
    let temp = tempfile::tempdir().unwrap();
    let (core, mut events) = app_core::spawn(temp.path()).await.unwrap();
    within(core.add_gtp_engine(entry)).await.unwrap();
    let setup = app_core::GameSetup {
        black: app_core::Player::Ai(app_core::AiPlayer::Gtp {
            engine_id: "gtp-a".into(),
            time: app_core::GtpTime::EngineDefault,
        }),
        white: app_core::Player::Human,
        auto_save: false,
        ..Default::default()
    };
    within(core.start_game(setup)).await.unwrap();
    within(async {
        loop {
            if let AppEvent::GtpConsoleLine(line) = events.recv().await.unwrap()
                && line.direction == GtpConsoleDirection::Sent
                && line.text.starts_with("genmove ")
            {
                break;
            }
        }
    })
    .await;
    let mut query = Box::pin(core.gtp_console_send("gtp-a".into(), "name".into()));
    assert!(
        query
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(
        within(core.pause_game()).await.unwrap().phase,
        app_core::GamePhase::Paused
    );
    assert!(matches!(
        within(query).await,
        Err(AppError::Gtp(GtpError::Session { .. }))
    ));
    within(core.shutdown()).await.unwrap();
}
