#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "common/game.rs"]
mod support;
use app_core::{
    AppCore, AppError, GameStart, GtpEngineEntry, GtpTime, KataGoPlay, MatchError, MatchPhase,
    MatchPlayer, MatchSetup, MatchView, NewGameOptions, Settings,
};
use common::within;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};
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
async fn add(app: &AppCore, directory: &Path, id: &str, args: &[&str]) {
    let mut entry = GtpEngineEntry {
        id: id.into(),
        name: id.into(),
        program: binary(),
        args: args.iter().map(|s| s.to_string()).collect(),
        working_dir: None,
        init_commands: vec![],
    };
    for (key, name) in [
        ("--launch-log", "launch"),
        ("--exit-log", "exit"),
        ("--command-log", "commands"),
    ] {
        entry.args.extend([
            key.into(),
            directory
                .join(format!("{id}-{name}.log"))
                .to_string_lossy()
                .into(),
        ]);
    }
    within(app.add_gtp_engine(entry)).await.unwrap();
}
fn side(id: &str) -> MatchPlayer {
    MatchPlayer::Gtp {
        engine_id: id.into(),
        time: GtpTime::EngineDefault,
    }
}
fn setup(a: &str, b: &str) -> MatchSetup {
    MatchSetup {
        a: side(a),
        b: side(b),
        start: GameStart::NewBoard(NewGameOptions {
            width: 9,
            height: 9,
            ..Default::default()
        }),
        auto_save: false,
        ..Default::default()
    }
}
async fn wait(app: &AppCore, games: u32) -> MatchView {
    for n in 1..=games {
        within(async {
            loop {
                let view = app.match_view().await.unwrap().unwrap();
                if view.completed >= n {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await;
    }
    within(async {
        loop {
            let view = app.match_view().await.unwrap().unwrap();
            if view.phase == MatchPhase::Completed {
                return view;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}
async fn plain() -> (tempfile::TempDir, AppCore) {
    let directory = tempfile::tempdir().unwrap();
    Settings {
        pondering: false,
        auto_quick_analyze_on_load: false,
        ..Default::default()
    }
    .save(directory.path())
    .await
    .unwrap();
    let (app, _) = app_core::spawn(directory.path()).await.unwrap();
    (directory, app)
}
#[tokio::test]
async fn same_gtp_entry_is_one_process_with_both_color_clocks_and_is_closed_after_match() {
    binary();
    let (directory, app) = plain().await;
    add(
        &app,
        directory.path(),
        "shared",
        &["--moves", "pass", "--final-score", "W+3.5"],
    )
    .await;
    let mut setup = setup("shared", "shared");
    setup.games = 2;
    setup.a = MatchPlayer::Gtp {
        engine_id: "shared".into(),
        time: GtpTime::PerMove(1),
    };
    setup.b = MatchPlayer::Gtp {
        engine_id: "shared".into(),
        time: GtpTime::PerMove(2),
    };
    within(app.start_match(setup)).await.unwrap();
    let view = wait(&app, 2).await;
    assert_eq!(view.a.white, 1);
    assert_eq!(view.b.white, 1);
    assert!(view.unscored.is_empty());
    assert_eq!(
        std::fs::read_to_string(directory.path().join("shared-launch.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("shared-exit.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let commands = std::fs::read_to_string(directory.path().join("shared-commands.log")).unwrap();
    assert!(commands.contains("time_settings 0 1 1"));
    assert!(commands.contains("time_settings 0 2 1"));
    assert!(commands.contains("genmove black"));
    assert!(commands.contains("genmove white"));
    assert!(commands.contains("final_score"));
    within(app.shutdown()).await.unwrap();
}
#[tokio::test]
async fn no_scorer_and_invalid_move_are_explicitly_unscored() {
    binary();
    let (directory, app) = plain().await;
    add(&app, directory.path(), "pass", &["--no-final-score"]).await;
    within(app.start_match(setup("pass", "pass")))
        .await
        .unwrap();
    let view = wait(&app, 1).await;
    assert_eq!(view.a.total + view.b.total, 0);
    assert_eq!(view.unscored.values().sum::<u32>(), 1);
    assert!(view.unscored.keys().any(|s| s.contains("未数子")));
    add(&app, directory.path(), "illegal", &["--moves", "Z99"]).await;
    within(app.start_match(setup("illegal", "illegal")))
        .await
        .unwrap();
    let view = wait(&app, 1).await;
    assert_eq!(view.unscored.values().sum::<u32>(), 1);
    assert!(view.unscored.keys().any(|s| s.contains("Z99")));
    assert_eq!(app.game_view().await.unwrap().unwrap().move_number, 0);
    within(app.shutdown()).await.unwrap();
}
#[tokio::test]
async fn gtp_and_nonmain_katago_share_current_position_and_fallback_counting() {
    binary();
    let h = support::harness(vec![]).await;
    add(
        &h.app,
        h.directory.path(),
        "gtp",
        &["--moves", "pass", "--no-final-score"],
    )
    .await;
    let mut profile = h.app.settings().await.unwrap().engine.unwrap();
    let script = h.directory.path().join("pass.json");
    std::fs::write(
        &script,
        r#"{"default":{"scoreLead":4.1,"candidates":[{"move":"pass"}]}}"#,
    )
    .unwrap();
    profile
        .extra_args
        .extend(["--play-script".into(), script.to_string_lossy().into()]);
    let settings = h.app.add_engine("Other".into(), profile).await.unwrap();
    let id = settings
        .engines
        .iter()
        .find(|e| e.name == "Other")
        .unwrap()
        .id
        .clone();
    within(h.app.close_engine()).await.unwrap();
    within(h.app.open_bytes(b"(;SZ[9]KM[6.5];B[aa])".to_vec()))
        .await
        .unwrap();
    within(h.app.step(1)).await.unwrap();
    let mut setup = setup("gtp", "gtp");
    setup.b = MatchPlayer::KataGo {
        engine_id: id,
        play: KataGoPlay {
            seconds: None,
            visits: Some(1),
            resign: None,
            ..Default::default()
        },
        wrn: 0.0,
        pda: 0.0,
    };
    setup.start = GameStart::CurrentPosition;
    within(h.app.start_match(setup)).await.unwrap();
    let view = wait(&h.app, 1).await;
    assert_eq!(view.a.black, 1);
    assert!(view.unscored.is_empty());
    let game = h.app.game_view().await.unwrap().unwrap();
    assert_eq!(game.move_number, 3);
    assert_eq!(game.result.as_deref(), Some("B+4.5"));
    let commands = std::fs::read_to_string(h.directory.path().join("gtp-commands.log")).unwrap();
    assert!(commands.contains("play black A9"));
    assert!(commands.contains("play white pass"));
    within(h.app.shutdown()).await.unwrap();
}
#[tokio::test]
async fn gtp_free_handicap_is_shared_and_play_now_is_rejected() {
    binary();
    let (directory, app) = plain().await;
    add(
        &app,
        directory.path(),
        "free",
        &["--handicap", "C3,G7", "--hang-command", "genmove"],
    )
    .await;
    let mut setup = setup("free", "free");
    setup.start = GameStart::FreeHandicap {
        options: NewGameOptions {
            width: 9,
            height: 9,
            handicap: 0,
            ..Default::default()
        },
        stones: 2,
    };
    within(app.start_match(setup)).await.unwrap();
    assert!(matches!(
        within(app.match_play_now()).await,
        Err(AppError::Match(MatchError::NotKataGo))
    ));
    assert!(matches!(
        within(app.gtp_console_send("free".into(), "play black A1".into())).await,
        Err(AppError::Gtp(
            app_core::GtpError::StateChangingCommand { .. }
        ))
    ));
    let game = app.game_view().await.unwrap().unwrap();
    assert_eq!(game.to_play, app_core::ColorView::White);
    assert_eq!(game.move_number, 0);
    let commands = std::fs::read_to_string(directory.path().join("free-commands.log")).unwrap();
    assert!(commands.contains("place_free_handicap 2"));
    assert!(commands.contains("set_free_handicap"));
    within(app.stop_match()).await.unwrap();
    within(app.shutdown()).await.unwrap();
}
