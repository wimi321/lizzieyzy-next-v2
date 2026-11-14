#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use app_core::{
    AiPlayer, AppConfig, AppCore, AppEvent, AppSubscription, ColorView, GameEndReason, GameError,
    GamePhase, GameSetup, GameStart, GameView, GtpConsoleDirection, GtpEngineEntry, GtpTime,
    NewGameOptions, Player, Settings,
};
use common::within;
use go_core::{Color, Move, Point};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

fn binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let output =
            std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
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
                (value["reason"] == "compiler-artifact" && value["target"]["name"] == "fake-gtp")
                    .then(|| value["executable"].as_str().map(PathBuf::from))
                    .flatten()
            })
            .next()
            .expect("fake-gtp executable")
    })
}

struct Harness {
    dir: tempfile::TempDir,
    app: AppCore,
    events: AppSubscription,
}
async fn harness(args: &[&str], timeout: Option<Duration>) -> Harness {
    let program = binary().to_owned();
    let dir = tempfile::tempdir().unwrap();
    let settings = Settings {
        pondering: false,
        cache_enabled: false,
        auto_quick_analyze_on_load: false,
        gtp_engines: vec![GtpEngineEntry {
            id: "gtp".into(),
            name: "GTP Test".into(),
            program,
            args: args.iter().map(|s| (*s).into()).collect(),
            working_dir: None,
            init_commands: vec![],
        }],
        ..Default::default()
    };
    settings.save(dir.path()).await.unwrap();
    let mut config = AppConfig::default();
    if let Some(timeout) = timeout {
        config.gtp_session.response_timeout = timeout;
    }
    let (app, events) = within(app_core::spawn_with_config(dir.path(), config))
        .await
        .unwrap();
    Harness { dir, app, events }
}
fn setup(time: GtpTime) -> GameSetup {
    GameSetup {
        black: Player::Human,
        white: Player::Ai(AiPlayer::Gtp {
            engine_id: "gtp".into(),
            time,
        }),
        start: GameStart::NewBoard(NewGameOptions {
            width: 9,
            height: 9,
            ..Default::default()
        }),
        ponder: false,
        auto_save: false,
        ..Default::default()
    }
}
async fn view(app: &AppCore) -> GameView {
    within(app.game_view()).await.unwrap().unwrap()
}
async fn wait_view(app: &AppCore, predicate: impl Fn(&GameView) -> bool) -> GameView {
    within(async {
        loop {
            let view = app.game_view().await.unwrap().unwrap();
            if predicate(&view) {
                return view;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}
async fn sent(app: &AppCore) -> Vec<String> {
    within(app.gtp_console("gtp".into()))
        .await
        .unwrap()
        .into_iter()
        .filter(|line| line.direction == GtpConsoleDirection::Sent)
        .map(|line| {
            let text = line.text.trim();
            match text.split_once(' ') {
                Some((id, rest)) if id.bytes().all(|b| b.is_ascii_digit()) => rest.to_owned(),
                _ => text.to_owned(),
            }
        })
        .collect()
}
async fn document(h: &mut Harness, node: u32) -> app_core::DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = h.events.recv().await.unwrap()
                && view.tree.current == node
            {
                return view;
            }
        }
    })
    .await
}
fn play(color: Color, x: u8, y: u8) -> Move {
    Move::Play(color, Point { x, y })
}

#[tokio::test]
async fn fixed_handicap_uses_document_points_and_supported_rules_before_genmove() {
    let mut h = harness(&["--moves", "pass", "--kata"], None).await;
    let mut options = setup(GtpTime::EngineDefault);
    options.start = GameStart::NewBoard(NewGameOptions {
        width: 9,
        height: 9,
        handicap: 4,
        komi: 0.5,
        ..Default::default()
    });
    within(h.app.start_game(options)).await.unwrap();
    let view = wait_view(&h.app, |v| v.move_number == 1).await;
    let board = document(&mut h, view.latest_node).await.board.unwrap();
    assert_eq!(board.stones.iter().flatten().count(), 4);
    let lines = sent(&h.app).await;
    let handicap = lines
        .iter()
        .find(|line| line.starts_with("set_free_handicap "))
        .unwrap();
    let mut points: Vec<_> = handicap.split_whitespace().skip(1).collect();
    points.sort();
    assert_eq!(points, ["C3", "C7", "G3", "G7"]);
    assert!(lines.contains(&"kata-set-rules chinese".into()));
    assert!(
        lines.iter().position(|line| line == handicap).unwrap()
            < lines
                .iter()
                .position(|line| line == "genmove white")
                .unwrap()
    );
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn free_handicap_accepts_engine_points_and_rejects_invalid_or_duplicate_points() {
    let mut h = harness(&["--handicap", "B2,H8", "--moves", "pass"], None).await;
    let mut options = setup(GtpTime::EngineDefault);
    options.start = GameStart::FreeHandicap {
        options: NewGameOptions {
            width: 9,
            height: 9,
            ..Default::default()
        },
        stones: 2,
    };
    within(h.app.start_game(options.clone())).await.unwrap();
    let view = wait_view(&h.app, |v| v.move_number == 1).await;
    let document = document(&mut h, view.latest_node).await;
    assert_eq!(document.game_info.handicap, Some(2));
    let board = document.board.unwrap();
    assert_eq!(board.stones[7 * 9 + 1], Some(ColorView::Black));
    assert_eq!(board.stones[9 + 7], Some(ColorView::Black));
    assert!(sent(&h.app).await.contains(&"place_free_handicap 2".into()));
    within(h.app.shutdown()).await.unwrap();
    for points in ["C3,C3", "Z99,G7", "C3"] {
        let h = harness(&["--handicap", points], None).await;
        assert!(within(h.app.start_game(options.clone())).await.is_err());
        assert!(within(h.app.game_view()).await.unwrap().is_none());
        within(h.app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn continuation_replays_main_path_and_each_opponent_move_once() {
    let h = harness(&["--moves", "D4"], None).await;
    within(h.app.open_bytes(b"(;SZ[9];B[aa];W[bb])".to_vec()))
        .await
        .unwrap();
    within(h.app.step(2)).await.unwrap();
    let mut options = setup(GtpTime::EngineDefault);
    options.start = GameStart::CurrentPosition;
    within(h.app.start_game(options)).await.unwrap();
    within(h.app.play(play(Color::Black, 2, 2))).await.unwrap();
    wait_view(&h.app, |v| v.move_number == 4).await;
    let lines = sent(&h.app).await;
    assert_eq!(
        lines
            .iter()
            .filter(|s| s.starts_with("play "))
            .cloned()
            .collect::<Vec<_>>(),
        ["play black A9", "play white B8", "play black C7"]
    );
    let tree = within(h.app.variation_tree()).await.unwrap();
    assert_eq!(
        tree.nodes.last().unwrap().mv,
        Some(play(Color::White, 3, 5).into())
    );
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn all_time_controls_emit_exact_commands_and_engine_default_emits_none() {
    for (time, expected, left) in [
        (GtpTime::PerMove(7), Some("time_settings 0 7 1"), None),
        (
            GtpTime::Byoyomi {
                main: 30,
                period: 5,
                stones: 2,
            },
            Some("time_settings 30 5 2"),
            Some("time_left white 30 0"),
        ),
        (
            GtpTime::Byoyomi {
                main: 0,
                period: 5,
                stones: 2,
            },
            Some("time_settings 0 5 2"),
            Some("time_left white 5 2"),
        ),
        (
            GtpTime::Fischer {
                main: 30,
                increment: 2,
            },
            Some("kata-time_settings fischer 30 2"),
            Some("time_left white 30 0"),
        ),
        (
            GtpTime::Absolute { main: 30 },
            Some("kata-time_settings absolute 30"),
            Some("time_left white 30 0"),
        ),
        (GtpTime::EngineDefault, None, None),
        (
            GtpTime::Custom(vec!["time_settings 0 4 1".into(), "echo custom".into()]),
            Some("time_settings 0 4 1"),
            None,
        ),
    ] {
        let h = harness(&["--kata", "--moves", "pass"], None).await;
        within(h.app.start_game(setup(time))).await.unwrap();
        within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
        wait_view(&h.app, |v| v.move_number == 2).await;
        let lines = sent(&h.app).await;
        let times: Vec<_> = lines
            .iter()
            .filter(|line| {
                line.starts_with("time_settings ") || line.starts_with("kata-time_settings ")
            })
            .collect();
        match expected {
            Some(expected) => assert_eq!(times, vec![expected]),
            None => assert!(times.is_empty()),
        }
        let lefts: Vec<_> = lines
            .iter()
            .filter(|line| line.starts_with("time_left "))
            .collect();
        match left {
            Some(left) => assert_eq!(lefts, vec![left]),
            None => assert!(lefts.is_empty()),
        }
        within(h.app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn fischer_and_absolute_require_advertised_kata_time_support() {
    for time in [
        GtpTime::Fischer {
            main: 10,
            increment: 1,
        },
        GtpTime::Absolute { main: 10 },
    ] {
        let h = harness(&[], None).await;
        assert!(matches!(
            within(h.app.start_game(setup(time))).await,
            Err(app_core::AppError::Game(GameError::GtpUnsupportedTime))
        ));
        assert!(
            !sent(&h.app)
                .await
                .iter()
                .any(|line| line.starts_with("kata-"))
        );
        assert!(within(h.app.game_view()).await.unwrap().is_none());
        within(h.app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn resignation_and_illegal_move_finish_without_fabricating_pass() {
    for coordinate in ["ReSiGn", "A9", "not-a-coordinate"] {
        let h = harness(&["--moves", coordinate], None).await;
        within(h.app.start_game(setup(GtpTime::EngineDefault)))
            .await
            .unwrap();
        within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
        let end = wait_view(&h.app, |v| v.phase == GamePhase::Finished).await;
        assert_eq!(end.move_number, 1);
        if coordinate == "ReSiGn" {
            assert_eq!(end.result.as_deref(), Some("B+R"));
            assert_eq!(end.end_reason, Some(GameEndReason::Resignation));
        } else {
            assert!(matches!(end.error, Some(GameError::GtpIllegalMove(_))));
            assert_eq!(end.end_reason, Some(GameEndReason::EngineError));
            assert!(end.result.is_none());
        }
        assert_eq!(within(h.app.variation_tree()).await.unwrap().nodes.len(), 2);
        within(h.app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn take_back_uses_undo_or_clears_and_replays_when_unavailable_or_rejected() {
    for flags in [vec![], vec!["--no-undo"], vec!["--fail-command", "undo"]] {
        let mut args = vec!["--moves", "D4"];
        args.extend(flags.iter().copied());
        let h = harness(&args, None).await;
        within(h.app.start_game(setup(GtpTime::EngineDefault)))
            .await
            .unwrap();
        within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
        wait_view(&h.app, |v| v.move_number == 2).await;
        let before = sent(&h.app).await.len();
        let undone = within(h.app.take_back()).await.unwrap();
        assert_eq!(undone.move_number, 0);
        let lines = sent(&h.app).await;
        let extra = &lines[before..];
        if flags.is_empty() {
            assert_eq!(extra, ["undo", "undo"]);
        } else {
            assert!(extra.contains(&"clear_board".into()));
        }
        assert_eq!(within(h.app.variation_tree()).await.unwrap().nodes.len(), 1);
        within(h.app.play(play(Color::Black, 1, 0))).await.unwrap();
        wait_view(&h.app, |v| v.move_number == 2).await;
        within(h.app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn final_score_after_two_passes_writes_result_and_missing_scorer_is_unscored() {
    for (flags, expected) in [
        (vec!["--final-score", "w+3.5"], Some("W+3.5")),
        (vec!["--no-final-score"], None),
    ] {
        let h = harness(&flags, None).await;
        within(h.app.start_game(setup(GtpTime::EngineDefault)))
            .await
            .unwrap();
        within(h.app.play(Move::Pass(Color::Black))).await.unwrap();
        let end = wait_view(&h.app, |v| v.phase == GamePhase::Finished).await;
        assert_eq!(end.move_number, 2);
        assert_eq!(end.result.as_deref(), expected);
        assert_eq!(
            end.end_reason,
            Some(if expected.is_some() {
                GameEndReason::Scored
            } else {
                GameEndReason::Unscored
            })
        );
        if expected.is_none() {
            assert_eq!(end.error, Some(GameError::Unscored));
        }
        let path = h.dir.path().join("result.sgf");
        within(h.app.save(path.clone())).await.unwrap();
        let saved = std::fs::read_to_string(path).unwrap();
        if let Some(expected) = expected {
            assert!(saved.contains(&format!("RE[{expected}]")));
        } else {
            assert!(!saved.contains("RE["));
        }
        within(h.app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn timeout_or_exit_finishes_without_automatic_restart() {
    for flag in ["--hang-command", "--exit-command"] {
        let h = harness(&[flag, "genmove"], Some(Duration::from_millis(100))).await;
        within(h.app.start_game(setup(GtpTime::EngineDefault)))
            .await
            .unwrap();
        within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
        let end = wait_view(&h.app, |v| v.phase == GamePhase::Finished).await;
        assert_eq!(end.end_reason, Some(GameEndReason::EngineError));
        assert!(matches!(end.error, Some(GameError::Gtp { .. })));
        assert_eq!(
            sent(&h.app)
                .await
                .iter()
                .filter(|s| *s == "protocol_version")
                .count(),
            1
        );
        assert_eq!(view(&h.app).await.move_number, 1);
        within(h.app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn pausing_an_inflight_genmove_reaps_process_and_preserves_position() {
    let h = harness(&["--hang-command", "genmove"], None).await;
    within(h.app.start_game(setup(GtpTime::EngineDefault)))
        .await
        .unwrap();
    within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
    within(async {
        loop {
            if sent(&h.app).await.contains(&"genmove white".into()) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    let paused = within(h.app.pause_game()).await.unwrap();
    assert_eq!(paused.phase, GamePhase::Paused);
    assert!(!paused.ai_thinking);
    let resumed = within(h.app.resume_game()).await.unwrap();
    assert!(resumed.ai_thinking);
    assert_eq!(resumed.latest_node, paused.latest_node);
    within(h.app.take_back()).await.unwrap();
    assert_eq!(view(&h.app).await.move_number, 0);
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn unavailable_final_score_falls_back_to_primary_katago_counting() {
    // Build preparation is outside the behavioral deadline.
    let program = fake_katago::binary_path().unwrap();
    let mut h = harness(&["--no-final-score"], None).await;
    let model = h.dir.path().join("model.bin");
    let config = h.dir.path().join("analysis.cfg");
    let script = h.dir.path().join("count.json");
    std::fs::write(&model, "fake").unwrap();
    std::fs::write(&config, "").unwrap();
    std::fs::write(
        &script,
        r#"{"default":{"scoreLead":2.1,"candidates":[{"move":"pass"}]}}"#,
    )
    .unwrap();
    within(h.app.configure_engine(Some(app_core::EngineProfile {
        program,
        model,
        config,
        extra_args: vec![
            "--play-script".into(),
            script.to_string_lossy().into_owned(),
            "--visits-per-sec".into(),
            "100000".into(),
        ],
        canonical_model_name: None,
    })))
    .await
    .unwrap();
    within(async {
        loop {
            if matches!(
                h.events.recv().await.unwrap(),
                AppEvent::EngineStatusChanged(app_core::EngineStatus::Ready { .. })
            ) {
                break;
            }
        }
    })
    .await;
    within(h.app.start_game(setup(GtpTime::EngineDefault)))
        .await
        .unwrap();
    within(h.app.play(Move::Pass(Color::Black))).await.unwrap();
    let end = wait_view(&h.app, |v| v.phase == GamePhase::Finished).await;
    assert_eq!(end.result.as_deref(), Some("B+2.5"));
    assert_eq!(end.end_reason, Some(GameEndReason::Scored));
    assert!(!sent(&h.app).await.iter().any(|line| line == "final_score"));
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn engine_exit_during_human_turn_is_not_restarted_for_next_move() {
    let h = harness(&["--exit-command", "echo"], None).await;
    within(h.app.start_game(setup(GtpTime::EngineDefault)))
        .await
        .unwrap();
    assert!(
        within(h.app.gtp_console_send("gtp".into(), "echo stop".into()))
            .await
            .is_err()
    );
    let ended = wait_view(&h.app, |v| v.phase == GamePhase::Finished).await;
    assert_eq!(ended.move_number, 0);
    assert_eq!(ended.end_reason, Some(GameEndReason::EngineError));
    assert!(matches!(ended.error, Some(GameError::Gtp { .. })));
    assert_eq!(
        sent(&h.app)
            .await
            .iter()
            .filter(|s| *s == "protocol_version")
            .count(),
        1
    );
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn cancelled_gtp_takeback_reapplies_time_control_to_new_process() {
    let h = harness(&["--hang-command", "genmove"], None).await;
    within(h.app.start_game(setup(GtpTime::Byoyomi {
        main: 30,
        period: 5,
        stones: 1,
    })))
    .await
    .unwrap();
    within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
    within(async {
        loop {
            if sent(&h.app).await.contains(&"genmove white".into()) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    within(h.app.take_back()).await.unwrap();
    within(h.app.play(play(Color::Black, 1, 0))).await.unwrap();
    within(async {
        loop {
            let lines = sent(&h.app).await;
            if lines.contains(&"genmove white".into()) {
                assert!(lines.contains(&"time_settings 30 5 1".into()));
                let left = lines
                    .iter()
                    .find(|s| s.starts_with("time_left white "))
                    .unwrap();
                let seconds: f64 = left.split_whitespace().nth(2).unwrap().parse().unwrap();
                assert!(seconds > 0.0 && seconds <= 30.0);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn process_exit_during_undo_keeps_failure_monitor_and_ends_the_game() {
    let h = harness(&["--moves", "D4", "--exit-command", "undo"], None).await;
    within(h.app.start_game(setup(GtpTime::EngineDefault)))
        .await
        .unwrap();
    within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
    wait_view(&h.app, |v| v.move_number == 2).await;
    assert!(within(h.app.take_back()).await.is_err());
    let ended = wait_view(&h.app, |v| v.phase == GamePhase::Finished).await;
    assert_eq!(ended.end_reason, Some(GameEndReason::EngineError));
    assert!(ended.result.is_none());
    assert!(matches!(ended.error, Some(GameError::Gtp { .. })));
    assert_eq!(
        sent(&h.app)
            .await
            .iter()
            .filter(|s| *s == "protocol_version")
            .count(),
        1
    );
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn changed_time_control_keeps_elapsed_time_across_pause_and_resume() {
    let h = harness(&["--kata", "--hang-command", "genmove"], None).await;
    within(h.app.start_game(setup(GtpTime::Absolute { main: 10 })))
        .await
        .unwrap();
    within(h.app.set_ai_player(
        Color::White,
        AiPlayer::Gtp {
            engine_id: "gtp".into(),
            time: GtpTime::Absolute { main: 100 },
        },
    ))
    .await
    .unwrap();
    within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
    within(async {
        loop {
            if sent(&h.app).await.contains(&"genmove white".into()) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::time::resume();
    within(h.app.pause_game()).await.unwrap();
    within(h.app.resume_game()).await.unwrap();
    let lines = within(async {
        loop {
            let lines = sent(&h.app).await;
            if lines.contains(&"genmove white".into()) {
                break lines;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(lines.contains(&"kata-time_settings absolute 100".into()));
    let left = lines
        .iter()
        .find(|line| line.starts_with("time_left white "))
        .unwrap();
    let seconds: u32 = left.split_whitespace().nth(2).unwrap().parse().unwrap();
    assert!(
        seconds < 100 && seconds > 0,
        "changed time was refunded: {left}"
    );
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn known_move_budget_extends_short_communication_timeout() {
    let h = harness(
        &["--hang-command", "genmove"],
        Some(Duration::from_millis(500)),
    )
    .await;
    within(h.app.start_game(setup(GtpTime::PerMove(300))))
        .await
        .unwrap();
    within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
    within(async {
        loop {
            if sent(&h.app).await.contains(&"genmove white".into()) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::time::resume();
    let paused = within(h.app.pause_game()).await.unwrap();
    assert_eq!(paused.phase, GamePhase::Paused);
    assert!(paused.error.is_none());
    within(h.app.shutdown()).await.unwrap();
}
