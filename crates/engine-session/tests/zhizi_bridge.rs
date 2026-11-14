#![allow(clippy::unwrap_used, clippy::expect_used)]
use engine_session::{
    AnalysisRequest, EngineHandle, EngineStatus, QueryEvent, QuerySubscription, SessionConfig,
    TransportSpec, spawn,
};
use katago_protocol::AnalysisResponse;
use serde_json::json;
use std::{
    collections::BTreeMap,
    future::Future,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::Duration,
};

#[track_caller]
fn within<T>(future: impl Future<Output = T>) -> impl Future<Output = T> {
    let caller = std::panic::Location::caller();
    async move {
        tokio::time::timeout(Duration::from_secs(5), future)
            .await
            .unwrap_or_else(|_| panic!("bridge safety deadline at {caller}"))
    }
}
fn fake() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap();
            let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
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
                .find_map(|line| {
                    let v: serde_json::Value = serde_json::from_str(line).ok()?;
                    (v["reason"] == "compiler-artifact" && v["target"]["name"] == "fake-gtp")
                        .then(|| v["executable"].as_str().map(PathBuf::from))
                        .flatten()
                })
                .unwrap()
        })
        .as_path()
}
fn start(extra: &[&str]) -> EngineHandle {
    let mut args = vec!["--kata".into()];
    args.extend(extra.iter().map(Into::into));
    spawn(
        TransportSpec::GtpBridge {
            inner: Box::new(TransportSpec::LocalProcess {
                program: fake().to_owned(),
                args,
                cwd: None,
                env: BTreeMap::new(),
            }),
        },
        SessionConfig {
            startup_timeout: Duration::from_secs(60),
            response_timeout: Duration::from_secs(60),
            max_restarts: 0,
            ..Default::default()
        },
    )
}
async fn ready(engine: &EngineHandle) {
    let mut status = engine.status();
    within(async {
        loop {
            let state = status.borrow_and_update().clone();
            match state {
                EngineStatus::Ready { version } => {
                    assert_eq!(version, "1.0-fake");
                    return;
                }
                EngineStatus::Failed { reason, .. } => panic!("{reason}"),
                _ => status.changed().await.unwrap(),
            }
        }
    })
    .await;
}
fn request(visits: u64) -> AnalysisRequest {
    serde_json::from_value(json!({"moves":[],"rules":"chinese","boardXSize":3,"boardYSize":2,"maxVisits":visits,"includeOwnership":true,"reportDuringSearchEvery":0.01})).unwrap()
}
async fn partial(query: &mut QuerySubscription) -> AnalysisResponse {
    within(async {
        match query.recv().await.unwrap() {
            QueryEvent::Partial(value) => value,
            other => panic!("expected partial, received {other:?}"),
        }
    })
    .await
}
async fn final_result(query: &mut QuerySubscription) -> AnalysisResponse {
    within(async {
        loop {
            match query.recv().await.unwrap() {
                QueryEvent::Final(value) => return value,
                QueryEvent::Partial(_) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
    })
    .await
}

#[tokio::test]
async fn realtime_root_max_visits_and_unsupported_extras() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("commands");
    let engine = start(&["--command-log", log.to_str().unwrap()]);
    ready(&engine).await;
    let mut input = request(250);
    input.include_policy = Some(true);
    input.include_moves_ownership = Some(true);
    let mut query = engine.submit(input).await.unwrap();
    let first = partial(&mut query).await;
    assert!(first.is_during_search);
    assert!(first.root_info.visits < 250);
    let last = final_result(&mut query).await;
    assert_eq!(last.root_info.visits, 300);
    assert_eq!(last.move_infos[0].visits, 150);
    assert!(!last.is_during_search);
    assert_eq!(last.ownership, Some(vec![0.25; 6]));
    assert!(last.policy.is_none());
    assert!(last.move_infos[0].ownership.is_none());
    engine.shutdown().await.unwrap();
    let commands = std::fs::read_to_string(log).unwrap();
    assert!(commands.contains("rectangular_boardsize 3 2"));
    assert!(commands.contains("kata-analyze B 10 ownership true rootInfo true"));
    assert!(commands.contains("stop\n"));
    assert!(commands.contains("quit\n"));
}

#[tokio::test]
async fn multiple_turns_replay_setup_and_black_perspective_exactly_once() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("commands");
    let engine = start(&["--command-log", log.to_str().unwrap()]);
    ready(&engine).await;
    let mut input = request(100);
    input.moves = serde_json::from_value(json!([["B", "B1"], ["W", "C2"]])).unwrap();
    input.initial_stones = Some(serde_json::from_value(json!([["B", "C1"]])).unwrap());
    input.analyze_turns = Some(vec![0, 1, 2]);
    // The remote actor overwrites this WHITE override with BLACK. The bridge
    // still converts GTP side-to-play values exactly once.
    input.override_settings =
        Some(serde_json::from_value(json!({"reportAnalysisWinratesAs":"WHITE"})).unwrap());
    let mut query = engine.submit(input).await.unwrap();
    for turn in 0..3 {
        let last = final_result(&mut query).await;
        assert_eq!(last.turn_number, turn);
        if turn == 1 {
            assert!((last.root_info.winrate - 0.35).abs() < 1e-10);
            assert_eq!(last.root_info.score_lead, -2.0);
            assert!((last.move_infos[0].winrate - 0.3).abs() < 1e-10);
            assert_eq!(last.move_infos[0].score_lead, -2.5);
            assert_eq!(last.ownership, Some(vec![-0.25; 6]));
        } else {
            assert_eq!(last.root_info.winrate, 0.65);
            assert_eq!(last.root_info.score_lead, 2.0);
        }
    }
    engine.shutdown().await.unwrap();
    let commands = std::fs::read_to_string(log).unwrap();
    assert_eq!(commands.matches("set_position B C1").count(), 3);
    assert_eq!(commands.matches("play B B1").count(), 2);
    assert_eq!(commands.matches("play W C2").count(), 1);
    assert!(!commands.contains("reportAnalysisWinratesAs"));
}

#[tokio::test]
async fn max_time_returns_last_report_then_stops() {
    let engine = start(&[]);
    ready(&engine).await;
    let mut input = request(u64::MAX);
    input.override_settings = Some(serde_json::from_value(json!({"maxTime":1.0})).unwrap());
    let mut query = engine.submit(input).await.unwrap();
    let first = partial(&mut query).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::time::resume();
    let last = final_result(&mut query).await;
    assert!(last.root_info.visits >= first.root_info.visits);
    assert!(last.root_info.visits < u64::MAX);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn queue_priority_fifo_and_cancellation() {
    let engine = start(&[]);
    ready(&engine).await;
    let mut blocker = engine.submit(request(u64::MAX)).await.unwrap();
    partial(&mut blocker).await;
    let mut low_input = request(u64::MAX);
    low_input.priority = Some(-10);
    let mut low = engine.submit(low_input).await.unwrap();
    let mut high_input = request(u64::MAX);
    high_input.priority = Some(20);
    let mut high = engine.submit(high_input.clone()).await.unwrap();
    let mut second_high = engine.submit(high_input).await.unwrap();
    let mut cancelled = engine.submit(request(u64::MAX)).await.unwrap();
    engine.terminate(cancelled.id().into()).await.unwrap();
    assert_eq!(within(cancelled.recv()).await, Some(QueryEvent::Terminated));
    engine.terminate(blocker.id().into()).await.unwrap();
    assert_eq!(within(blocker.recv()).await, Some(QueryEvent::Terminated));
    partial(&mut high).await;
    // Timeouts never prove absence: select against the next real report from
    // the active high-priority query, making wrong scheduling observable.
    tokio::select! { value = second_high.recv() => panic!("FIFO violated: {value:?}"), value = low.recv() => panic!("priority violated: {value:?}"), _ = partial(&mut high) => {} }
    engine.terminate(high.id().into()).await.unwrap();
    partial(&mut second_high).await;
    engine.terminate(second_high.id().into()).await.unwrap();
    partial(&mut low).await;
    engine.terminate(low.id().into()).await.unwrap();
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn setup_without_capability_is_query_error_and_next_query_works() {
    let engine = start(&["--no-set-position"]);
    ready(&engine).await;
    let mut input = request(100);
    input.initial_stones = Some(serde_json::from_value(json!([["B", "B1"]])).unwrap());
    let mut query = engine.submit(input).await.unwrap();
    assert!(matches!(
        within(query.recv()).await,
        Some(QueryEvent::Failed(engine_session::QueryError::Bridge(
            engine_session::BridgeError::SetupUnsupported
        )))
    ));
    let mut next = engine.submit(request(100)).await.unwrap();
    assert_eq!(final_result(&mut next).await.root_info.visits, 100);
    engine.shutdown().await.unwrap();
}
