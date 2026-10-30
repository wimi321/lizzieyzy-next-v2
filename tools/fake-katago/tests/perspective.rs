#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Engine, query};
use serde_json::{Value, json};

fn start(args: &[&str]) -> Engine {
    let path = fake_katago::binary_path().expect("build fake-katago");
    Engine::spawn_at(path, args)
}

fn request() -> Value {
    let mut request = query("perspective");
    request["moves"] = json!([["B", "D4"], ["W", "Q16"], ["B", "pass"]]);
    request["analyzeTurns"] = json!([0, 1, 2, 3]);
    request["includeOwnership"] = json!(true);
    request["includePolicy"] = json!(true);
    request
}

fn results(args: &[&str], request: &Value) -> Vec<Value> {
    let mut engine = start(args);
    engine.send(request);
    let count = request["analyzeTurns"].as_array().map_or(1, Vec::len);
    let replies = (0..count).map(|_| engine.recv()).collect();
    engine.finish();
    replies
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
}

fn assert_inverted_info(black: &Value, white: &Value) {
    for field in ["winrate", "rawWinrate", "lcb"] {
        if let Some(value) = black[field].as_f64() {
            close(white[field].as_f64().unwrap(), 1.0 - value);
        }
    }
    for field in [
        "scoreMean",
        "scoreLead",
        "scoreSelfplay",
        "utility",
        "utilityLcb",
        "rawLead",
        "rawScoreSelfplay",
    ] {
        if let Some(value) = black[field].as_f64() {
            close(white[field].as_f64().unwrap(), -value);
        }
    }
    for field in [
        "currentPlayer",
        "move",
        "prior",
        "order",
        "pv",
        "scoreStdev",
        "rawScoreSelfplayStdev",
        "rawNoResultProb",
        "rawStWrError",
        "rawStScoreError",
    ] {
        assert_eq!(black[field], white[field], "{field}");
    }
}

fn assert_inverted(black: &Value, white: &Value) {
    assert_inverted_info(&black["rootInfo"], &white["rootInfo"]);
    let black_moves = black["moveInfos"].as_array().unwrap();
    let white_moves = white["moveInfos"].as_array().unwrap();
    assert_eq!(black_moves.len(), white_moves.len());
    for (black, white) in black_moves.iter().zip(white_moves) {
        assert_inverted_info(black, white);
    }
    let black_ownership = black["ownership"].as_array().unwrap();
    let white_ownership = white["ownership"].as_array().unwrap();
    assert_eq!(black_ownership.len(), white_ownership.len());
    for (black, white) in black_ownership.iter().zip(white_ownership) {
        close(white.as_f64().unwrap(), -black.as_f64().unwrap());
    }
    assert_eq!(black["policy"], white["policy"]);
}

#[test]
fn unset_perspective_uses_each_turn_player_and_black_override_preserves_legacy() {
    let request = request();
    let legacy = results(&[], &request);
    let side_to_move = results(&["--default-perspective-to-play"], &request);
    let overridden = results(
        &[
            "-override-config",
            "otherSetting=ignored,reportAnalysisWinratesAs=BLACK",
            "--default-perspective-to-play",
        ],
        &request,
    );
    assert_eq!(legacy, overridden);
    for (turn, (black, side)) in legacy.iter().zip(&side_to_move).enumerate() {
        assert_eq!(black["turnNumber"], turn);
        assert_eq!(black["rootInfo"]["visits"], side["rootInfo"]["visits"]);
        if turn % 2 == 0 {
            assert_eq!(black, side);
        } else {
            assert_eq!(side["rootInfo"]["currentPlayer"], "W");
            assert_ne!(black["rootInfo"]["winrate"], side["rootInfo"]["winrate"]);
            assert_inverted(black, side);
        }
    }
}

#[test]
fn single_override_config_supports_multiple_settings_and_all_perspectives() {
    let request = request();
    let black = results(&[], &request);
    let white = results(
        &[
            "-override-config",
            "numSearchThreads=2,reportAnalysisWinratesAs=WHITE",
        ],
        &request,
    );
    for (black, white) in black.iter().zip(&white) {
        assert_inverted(black, white);
    }
    let black_again = results(
        &[
            "--default-perspective-to-play",
            "-override-config",
            "numSearchThreads=2,reportAnalysisWinratesAs=BLACK,nnCacheSizePowerOfTwo=18",
        ],
        &request,
    );
    assert_eq!(black, black_again);
    assert_eq!(
        results(&["--default-perspective-to-play"], &request),
        results(
            &["-override-config", "reportAnalysisWinratesAs=SIDETOMOVE"],
            &request
        )
    );
}

#[test]
fn partial_and_terminated_results_also_use_white_to_play_perspective() {
    let mut request = request();
    request["moves"] = json!([["B", "D4"]]);
    request["analyzeTurns"] = json!([1]);
    let black = results(&[], &request).remove(0);
    request["maxVisits"] = json!(1_000_000_000);
    request["firstReportDuringSearchAfter"] = json!(0.001);
    let mut engine = start(&["--default-perspective-to-play"]);
    engine.send(&request);
    let partial = engine.recv();
    assert_eq!(partial["isDuringSearch"], true);
    assert_inverted(&black, &partial);
    engine.send(&json!({"id":"cancel", "action":"terminate", "terminateId":"perspective"}));
    assert_eq!(engine.recv()["action"], "terminate");
    let final_result = engine.recv();
    assert_eq!(final_result["isDuringSearch"], false);
    assert_inverted(&black, &final_result);
    engine.finish();
}

#[test]
fn malformed_config_override_is_rejected_with_a_readable_error() {
    let path = fake_katago::binary_path().unwrap();
    for value in [
        "invalid",
        "=BLACK",
        "key=",
        "reportAnalysisWinratesAs=unknown",
    ] {
        let output = std::process::Command::new(&path)
            .args(["analysis", "-override-config", value])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(!output.stderr.is_empty());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn repeated_command_line_arguments_are_rejected_like_katago_value_args() {
    let path = fake_katago::binary_path().expect("build fake-katago");
    for (argument, value) in [
        ("-override-config", Some("reportAnalysisWinratesAs=BLACK")),
        ("-config", Some("unused.cfg")),
        ("-model", Some("unused.bin.gz")),
        ("--visits-per-sec", Some("1000")),
        ("--time-scale", Some("1")),
        ("--startup-delay-ms", Some("0")),
        ("--startup-gate-file", Some("unused-gate")),
        ("--crash-after-queries", Some("1")),
        ("--crash-after-ms", Some("1")),
        ("--hang-after-queries", Some("1")),
        ("--fault-once-file", Some("unused-marker")),
        ("--garbage-every", Some("1")),
        ("--threads", Some("1")),
        ("--fail-startup", None),
        ("--default-perspective-to-play", None),
    ] {
        let mut command = std::process::Command::new(&path);
        command.arg("analysis");
        for _ in 0..2 {
            command.arg(argument);
            if let Some(value) = value {
                command.arg(value);
            }
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{argument}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("Argument already set"), "{stderr}");
        assert!(stderr.contains(argument), "{stderr}");
        assert!(output.stdout.is_empty());
    }
}
