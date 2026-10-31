#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AnalysisView, AppConfig, AppCore, AppEvent, AppSubscription, EngineProfile, EngineStatus,
};
use engine_session::{AnalysisRequest, QueryEvent, SessionConfig, TransportSpec};
use go_core::{Color, Move};
use std::{future::Future, path::Path, time::Duration};
use tempfile::TempDir;
use tokio::time::timeout;

const SAFETY: Duration = Duration::from_secs(5);

async fn within<T>(future: impl Future<Output = T>) -> T {
    timeout(SAFETY, future)
        .await
        .expect("operation exceeded safety deadline")
}

fn profile(directory: &Path, args: &[&str]) -> EngineProfile {
    let model = directory.join("test model.bin");
    let config = directory.join("user config.cfg");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&config, "numAnalysisThreads = 1\n").unwrap();
    EngineProfile {
        canonical_model_name: None,
        program: fake_katago::binary_path().unwrap(),
        model,
        config,
        extra_args: args.iter().map(|arg| (*arg).into()).collect(),
    }
}

async fn app(directory: &Path, max_visits: u64) -> (AppCore, AppSubscription) {
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = max_visits;
    config.analysis.live.report_interval = Duration::from_millis(10);
    // No timeout fault is under test. Keep deadlines beyond the outer safety net.
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    within(app_core::spawn_with_config(directory, config))
        .await
        .unwrap()
}

async fn event(output: &mut AppSubscription) -> AppEvent {
    within(output.recv())
        .await
        .expect("app event stream closed")
}

async fn document(output: &mut AppSubscription, revision: u64) -> app_core::DocumentView {
    within(async {
        loop {
            match output.recv().await.unwrap() {
                AppEvent::DocumentChanged {
                    revision: actual,
                    view,
                } => {
                    assert_eq!(actual, revision);
                    return view;
                }
                AppEvent::AnalysisUpdated(value) => {
                    panic!("analysis arrived before document: {value:?}")
                }
                _ => {}
            }
        }
    })
    .await
}

async fn final_analysis(output: &mut AppSubscription, revision: u64, node: u32) -> AnalysisView {
    within(async {
        loop {
            match output.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(value) => {
                    assert_eq!((value.revision, value.node_id), (revision, node));
                    if !value.is_during_search {
                        return value;
                    }
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed {
                    reason,
                    stderr_tail,
                }) => panic!("engine failed: {reason} {stderr_tail:?}"),
                _ => {}
            }
        }
    })
    .await
}

async fn ready(output: &mut AppSubscription) {
    within(async {
        loop {
            match output.recv().await.expect("app event stream closed") {
                AppEvent::EngineStatusChanged(EngineStatus::Ready { version }) => {
                    assert_eq!(version, "1.16.5-fake");
                    return;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed {
                    reason,
                    stderr_tail,
                }) => {
                    panic!("engine failed before Ready: {reason} {stderr_tail:?}");
                }
                _ => {}
            }
        }
    })
    .await;
}

async fn failed(output: &mut AppSubscription) -> (String, Vec<String>) {
    within(async {
        loop {
            if let AppEvent::EngineStatusChanged(EngineStatus::Failed {
                reason,
                stderr_tail,
            }) = output.recv().await.unwrap()
            {
                return (reason, stderr_tail);
            }
        }
    })
    .await
}

#[tokio::test]
async fn corpus_root_then_twenty_steps_never_deliver_stale_analysis() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), &["--visits-per-sec", "100000"]);
    let (handle, mut output) = app(directory.path(), 100).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    // Use the checked-in corpus and extend its main line with legal passes so
    // every one of the twenty Step commands changes the focus and revision.
    let mut collection = sgf::parse(include_bytes!("../../../fixtures/sgf/basic-19.sgf")).unwrap();
    let mut source = document::GameDocument::from_sgf(&collection.games[0]).unwrap();
    source.apply(document::Command::Step(i32::MAX)).unwrap();
    for _ in 0..20 {
        let color = source
            .position_at(source.current())
            .unwrap()
            .board
            .to_play();
        source
            .apply(document::Command::Play(Move::Pass(color)))
            .unwrap();
    }
    collection.games[0] = source.to_sgf();
    let root_revision = within(handle.open_bytes(sgf::serialize(&collection).into_bytes()))
        .await
        .unwrap();
    let root = document(&mut output, root_revision).await;
    final_analysis(&mut output, root_revision, root.tree.root).await;
    // Leave a previous-focus result unread before the burst. A FIFO event
    // channel would leak it after the final acknowledged navigation command.
    within(handle.apply(document::Command::SetComment("seed unread result".into())))
        .await
        .unwrap();
    let mut latest = 0;
    for _ in 0..20 {
        latest = within(handle.step(1)).await.unwrap();
    }
    assert_eq!(latest, root_revision + 21);
    let last_document = document(&mut output, latest).await;
    assert_eq!(last_document.board.unwrap().move_number, 20);
    final_analysis(&mut output, latest, last_document.tree.current).await;
    within(handle.shutdown()).await.unwrap();
    while let Some(value) = within(output.recv()).await {
        if let AppEvent::AnalysisUpdated(value) = value {
            assert_eq!(value.revision, latest);
        }
    }
}

#[tokio::test]
async fn missing_model_reports_reason_and_does_not_disable_navigation() {
    let directory = TempDir::new().unwrap();
    let mut engine = profile(directory.path(), &[]);
    engine.model = directory.path().join("missing-model.bin");
    let (handle, mut output) = app(directory.path(), 10).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    let (reason, _) = failed(&mut output).await;
    assert!(
        reason.contains("engine model") && reason.contains("missing-model.bin"),
        "{reason}"
    );
    let revision =
        within(handle.open_bytes(include_bytes!("../../../fixtures/sgf/basic-19.sgf").to_vec()))
            .await
            .unwrap();
    document(&mut output, revision).await;
    let revision = within(handle.step(1)).await.unwrap();
    assert_eq!(
        document(&mut output, revision)
            .await
            .board
            .unwrap()
            .move_number,
        1
    );
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn startup_failure_preserves_stderr_during_navigation_and_can_retry() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), &["--fail-startup"]);
    let (handle, mut output) = app(directory.path(), 10).await;
    within(handle.configure_engine(Some(engine.clone())))
        .await
        .unwrap();
    let (_, stderr) = failed(&mut output).await;
    assert!(stderr.iter().any(|line| line.contains("模型找不到")));
    let revision = within(handle.play(Move::Pass(Color::Black))).await.unwrap();
    document(&mut output, revision).await;
    let (_, stderr) = failed(&mut output).await;
    assert!(stderr.iter().any(|line| line.contains("模型找不到")));
    within(handle.configure_engine(Some(engine))).await.unwrap();
    // A new failure proves an unchanged but failed profile was retried.
    let (_, stderr) = failed(&mut output).await;
    assert!(stderr.iter().any(|line| line.contains("模型找不到")));
    within(handle.shutdown()).await.unwrap();
}

async fn direct_response(
    profile: &EngineProfile,
    to_play_default: bool,
) -> katago_protocol::AnalysisResponse {
    // The baseline process intentionally omits app-core's forced override.
    let mut args = vec![
        "analysis".into(),
        "-config".into(),
        profile.config.as_os_str().into(),
        "-model".into(),
        profile.model.as_os_str().into(),
    ];
    if to_play_default {
        args.push("--default-perspective-to-play".into());
    }
    let engine = engine_session::spawn(
        TransportSpec::LocalProcess {
            program: fake_katago::binary_path().unwrap(),
            args,
            cwd: None,
            env: Default::default(),
        },
        SessionConfig {
            write_timeout: Duration::from_secs(60),
            shutdown_timeout: Duration::from_secs(60),
            ..SessionConfig::default()
        },
    );
    let request: AnalysisRequest = serde_json::from_value(serde_json::json!({
        "moves": [["B", "pass"]], "initialStones": [], "initialPlayer": "B",
        "rules": "chinese", "komi": 7.5, "boardXSize": 19, "boardYSize": 19,
        "maxVisits": 20, "includeOwnership": true, "includePolicy": false,
    }))
    .unwrap();
    let mut query = within(engine.submit(request)).await.unwrap();
    let response = within(async {
        loop {
            match query.recv().await {
                Some(QueryEvent::Final(response)) => return response,
                Some(QueryEvent::Partial(_)) => {}
                other => panic!("unexpected direct response: {other:?}"),
            }
        }
    })
    .await;
    within(engine.shutdown()).await.unwrap();
    response
}

#[tokio::test]
async fn merged_user_override_reaches_ready_and_keeps_white_to_play_values_black() {
    let directory = TempDir::new().unwrap();
    let engine = profile(
        directory.path(),
        &[
            "--default-perspective-to-play",
            "-override-config",
            "numSearchThreads=2",
        ],
    );
    let black = direct_response(&engine, false).await;
    let white = direct_response(&engine, true).await;
    assert_ne!(black.root_info.winrate, white.root_info.winrate);
    assert_eq!(white.root_info.winrate, 1.0 - black.root_info.winrate);
    let (handle, mut output) = app(directory.path(), 20).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    ready(&mut output).await;
    let revision = within(handle.play(Move::Pass(Color::Black))).await.unwrap();
    let board = document(&mut output, revision).await;
    assert_eq!(board.board.unwrap().to_play, app_core::ColorView::White);
    let actual = final_analysis(&mut output, revision, board.tree.current).await;
    let mut reference = document::GameDocument::new();
    reference
        .apply(document::Command::Play(Move::Pass(Color::Black)))
        .unwrap();
    let position = reference.position_at(reference.current()).unwrap();
    let expected = AnalysisView::from_snapshot(
        analysis::AnalysisSnapshot {
            revision,
            node_id: document::NodeId(board.tree.current),
            key: position.analysis_key(),
            response: black,
            source: analysis::SnapshotSource::Live,
            visits_per_second: None,
        },
        position.board.size(),
        position.board.to_play(),
    )
    .unwrap();
    assert_eq!(actual, expected);
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn comment_revision_reuses_completed_query_and_pause_clears_unread_output() {
    let directory = TempDir::new().unwrap();
    // A second query would crash the engine: comment-only changes must just
    // republish the first completed result under the new revision.
    let engine = profile(directory.path(), &["--crash-after-queries", "2"]);
    let (handle, mut output) = app(directory.path(), 20).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    let initial = final_analysis(&mut output, 0, 0).await;
    let revision = within(handle.apply(document::Command::SetComment("hello".into())))
        .await
        .unwrap();
    document(&mut output, revision).await;
    let mut updated = final_analysis(&mut output, revision, 0).await;
    updated.revision = initial.revision;
    assert_eq!(updated, initial);
    within(handle.set_pondering(false)).await.unwrap();
    let revision = within(handle.apply(document::Command::SetComment("paused".into())))
        .await
        .unwrap();
    document(&mut output, revision).await;
    // Swap profile while paused, which also tests persistent pause across sessions.
    let engine = profile(directory.path(), &[]);
    within(handle.configure_engine(Some(engine))).await.unwrap();
    within(handle.set_pondering(true)).await.unwrap();
    final_analysis(&mut output, revision, 0).await;
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn live_analysis_view_reports_search_speed_and_preserves_it_after_completion() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), &["--visits-per-sec", "1000"]);
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1500;
    config.analysis.live.report_interval = Duration::from_millis(100);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    let (handle, mut output) = within(app_core::spawn_with_config(directory.path(), config))
        .await
        .unwrap();
    within(handle.configure_engine(Some(engine))).await.unwrap();

    let partial = within(async {
        loop {
            match output.recv().await.expect("app event stream closed") {
                AppEvent::AnalysisUpdated(view) => {
                    assert_eq!((view.revision, view.node_id), (0, 0));
                    assert!(view.is_during_search, "no speed before the search ended");
                    if let Some(speed) = view.visits_per_second {
                        assert!(speed.is_finite() && speed > 0.0, "{speed}");
                        return view;
                    }
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed {
                    reason,
                    stderr_tail,
                }) => panic!("engine failed: {reason} {stderr_tail:?}"),
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(partial.source, app_core::AnalysisSource::Live);

    let finished = final_analysis(&mut output, 0, 0).await;
    assert_eq!(finished.visits, 1500);
    let speed = finished.visits_per_second.expect("final lost search speed");
    assert!(speed.is_finite() && speed > 0.0, "{speed}");
    // Reading retained analysis must not present an earlier search's speed as
    // a measurement for cache or whole-game results.
    let stored = within(handle.whole_game_result(0)).await.unwrap().unwrap();
    assert_eq!(stored.source, app_core::AnalysisSource::Cache);
    assert_eq!(stored.visits_per_second, None);

    let revision = within(handle.apply(document::Command::SetComment("completed".into())))
        .await
        .unwrap();
    document(&mut output, revision).await;
    let mut republished = final_analysis(&mut output, revision, 0).await;
    republished.revision = finished.revision;
    assert_eq!(republished, finished);
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn crash_once_restarts_and_a_new_document_cannot_receive_the_previous_game_result() {
    let directory = TempDir::new().unwrap();
    let marker = directory.path().join("crashed-once");
    let gate = directory.path().join("startup-gate");
    let engine = profile(
        directory.path(),
        &[
            "--crash-after-queries",
            "1",
            "--fault-once-file",
            marker.to_str().unwrap(),
            "--startup-gate-file",
            gate.to_str().unwrap(),
        ],
    );
    let (handle, mut output) = app(directory.path(), 20).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    let first = within(handle.open_bytes(b"(;SZ[9]AB[aa])".to_vec()))
        .await
        .unwrap();
    let second = within(handle.open_bytes(b"(;SZ[9]AW[bb])".to_vec()))
        .await
        .unwrap();
    assert!(second > first);
    document(&mut output, second).await;
    std::fs::write(&gate, "ready").unwrap();
    let result = final_analysis(&mut output, second, 0).await;
    assert_eq!(result.ownership.as_ref().unwrap().len(), 81);
    assert!(marker.is_file());
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn dropping_last_handle_closes_output_even_with_subscription_alive() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), &[]);
    // Keep the search finite even if shutdown closes stdin before the queued
    // terminate is flushed; this test must never rely on a kill deadline.
    let (handle, mut output) = app(directory.path(), 200).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    loop {
        if matches!(event(&mut output).await, AppEvent::AnalysisUpdated(_)) {
            break;
        }
    }
    let clone = handle.clone();
    drop(handle);
    within(clone.settings()).await.unwrap();
    drop(clone);
    within(async { while output.recv().await.is_some() {} }).await;
}

#[tokio::test]
async fn reapplying_profile_is_idempotent_until_model_metadata_changes() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), &["--crash-after-queries", "2"]);
    let (handle, mut output) = app(directory.path(), 20).await;
    within(handle.configure_engine(Some(engine.clone())))
        .await
        .unwrap();
    final_analysis(&mut output, 0, 0).await;
    within(handle.configure_engine(Some(engine.clone())))
        .await
        .unwrap();
    let revision = within(handle.apply(document::Command::SetComment("same engine".into())))
        .await
        .unwrap();
    document(&mut output, revision).await;
    final_analysis(&mut output, revision, 0).await;
    // This must create a fresh session/coordinator even though paths and args
    // are identical. The old completed coordinator would emit nothing here;
    // submitting a second query to the old session would deliberately crash.
    std::fs::write(&engine.model, "replacement model with a different size").unwrap();
    within(handle.configure_engine(Some(engine))).await.unwrap();
    final_analysis(&mut output, revision, 0).await;
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn navigating_out_of_an_invalid_record_resumes_valid_analysis() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), &[]);
    let (handle, mut output) = app(directory.path(), 20).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    let revision = within(handle.open_bytes(b"(;SZ[9](;B[aa];W[aa])(;B[bb]))".to_vec()))
        .await
        .unwrap();
    document(&mut output, revision).await;
    final_analysis(&mut output, revision, 0).await;
    let revision = within(handle.step(2)).await.unwrap();
    let invalid = document(&mut output, revision).await;
    assert!(invalid.board.is_none());
    assert!(invalid.position_error.is_some());
    let revision = within(handle.go_to(3)).await.unwrap();
    let valid = document(&mut output, revision).await;
    assert!(valid.board.is_some());
    final_analysis(&mut output, revision, valid.tree.current).await;
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn switching_board_dimensions_and_player_updates_candidate_and_pv_context() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), &[]);
    let (handle, mut output) = app(directory.path(), 20).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    for (sgf, width, height, to_play) in [
        ("(;SZ[9:13]PL[W])", 9, 13, Color::White),
        ("(;SZ[13:9]PL[B])", 13, 9, Color::Black),
        ("(;SZ[19]PL[W])", 19, 19, Color::White),
    ] {
        let revision = within(handle.open_bytes(sgf.as_bytes().to_vec()))
            .await
            .unwrap();
        let doc = document(&mut output, revision).await;
        let board = doc.board.unwrap();
        assert_eq!((board.width, board.height), (width, height));
        assert_eq!(board.to_play, to_play.into());
        let view = final_analysis(&mut output, revision, doc.tree.current).await;
        assert_eq!(view.ownership.unwrap().len(), board.stones.len());
        assert!(!view.candidates.is_empty());
        let size = go_core::BoardSize::new(width, height).unwrap();
        for candidate in view.candidates {
            let first_move = match candidate.point {
                Some(point) => {
                    let point = go_core::Point {
                        x: point.x,
                        y: point.y,
                    };
                    assert_eq!(point.to_gtp(size).unwrap(), candidate.coordinate);
                    Move::Play(to_play, point)
                }
                None => {
                    assert_eq!(candidate.coordinate, "pass");
                    Move::Pass(to_play)
                }
            };
            assert_eq!(candidate.pv.first(), Some(&first_move.into()));
            let mut expected_color = to_play;
            for mv in candidate.pv {
                match mv {
                    app_core::MoveView::Play { color, point } => {
                        assert_eq!(color, expected_color.into());
                        assert!(point.x < width && point.y < height);
                    }
                    app_core::MoveView::Pass { color } => {
                        assert_eq!(color, expected_color.into());
                    }
                }
                expected_color = expected_color.opponent();
            }
        }
    }
    within(handle.shutdown()).await.unwrap();
}
