#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AppConfig, AppCore, AppError, AppEvent, AppSubscription, BoardSyncError, BoardSyncOptions,
    BoardSyncPhase, ColorView, DocumentView, EngineProfile, EngineStatus, MoveView, PointView,
};
use serde_json::Value;
use std::{
    future::Future,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::Duration,
};
use tempfile::TempDir;
use tokio::time::timeout;

async fn within<T>(future: impl Future<Output = T>) -> T {
    timeout(Duration::from_secs(5), future)
        .await
        .expect("board-sync safety deadline")
}

fn fake_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap();
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let output = Command::new(cargo)
                .args(["build", "--locked", "--manifest-path"])
                .arg(root.join("tools/fake-readboard/Cargo.toml"))
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
                    let value: Value = serde_json::from_str(line).ok()?;
                    if value["reason"] == "compiler-artifact"
                        && value["target"]["name"] == "fake-readboard"
                    {
                        value["executable"].as_str().map(PathBuf::from)
                    } else {
                        None
                    }
                })
                .next()
                .expect("cargo executable artifact")
        })
        .as_path()
}

fn helper(directory: &Path, replay: &str) -> PathBuf {
    let binary = fake_binary();
    let resource = directory.join("readboard");
    std::fs::create_dir_all(&resource).unwrap();
    let target = resource.join("readboard.exe");
    std::fs::copy(binary, &target).unwrap();
    std::fs::write(resource.join("replay.txt"), replay).unwrap();
    target
}

// Reconstructed wire frames, matching the pinned upstream golden examples.
// Each fixture exercises the production stdio process and decoder, not an injected snapshot.
fn frame(stones: &[(usize, u8)], number: Option<u32>) -> String {
    let mut cells = [0u8; 25];
    for &(index, stone) in stones {
        cells[index] = stone;
    }
    let mut output = String::from("lastMoveSource redBlueMarker\n");
    if let Some(number) = number {
        output.push_str(&format!("syncPlatform fox\nfoxMoveNumber {number}\n"));
    }
    for row in cells.chunks(5) {
        output.push_str("re=");
        output.push_str(&row.iter().map(u8::to_string).collect::<Vec<_>>().join(","));
        output.push('\n');
    }
    output.push_str("end\n");
    output
}

fn two_moves() -> String {
    format!(
        "start 5 5\nsync\n{}{}{}",
        frame(&[], Some(0)),
        frame(&[(6, 3)], Some(1)),
        frame(&[(6, 1), (18, 4)], Some(2))
    )
}

async fn app(directory: &Path) -> (AppCore, AppSubscription) {
    within(app_core::spawn(directory.join("settings")))
        .await
        .unwrap()
}

async fn synced(output: &mut AppSubscription, number: u32) -> DocumentView {
    within(async {
        let mut latest = None;
        loop {
            match output.recv().await.expect("application stream closed") {
                AppEvent::DocumentChanged { view, .. } => latest = Some(view),
                AppEvent::BoardSyncStatusChanged(status) => {
                    assert!(
                        status.error.is_none(),
                        "unexpected sync error: {:?}",
                        status.error
                    );
                    if status.phase == BoardSyncPhase::Syncing
                        && status.remote_move_number == Some(number)
                    {
                        return latest.expect("document must precede status");
                    }
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("engine failed: {reason}")
                }
                _ => {}
            }
        }
    })
    .await
}

async fn saved_document(handle: &AppCore, directory: &Path) -> document::GameDocument {
    let path = directory.join("result.sgf");
    within(handle.save(&path)).await.unwrap();
    let collection = sgf::parse(&std::fs::read(path).unwrap()).unwrap();
    document::GameDocument::from_sgf(&collection.games[0]).unwrap()
}

#[tokio::test]
async fn starts_and_appends_each_verified_snapshot_as_a_move() {
    let directory = TempDir::new().unwrap();
    let program = helper(directory.path(), &two_moves());
    let (handle, mut output) = app(directory.path()).await;
    let started = within(handle.start_board_sync(program, BoardSyncOptions::default()))
        .await
        .unwrap();
    assert_eq!(started.phase, BoardSyncPhase::Starting);
    let view = synced(&mut output, 2).await;
    let board = view.board.unwrap();
    assert_eq!(board.move_number, 2);
    assert_eq!(board.stones[6], Some(ColorView::Black));
    assert_eq!(board.stones[18], Some(ColorView::White));
    assert_eq!(board.to_play, ColorView::Black);
    let tree = within(handle.variation_tree()).await.unwrap();
    assert_eq!(tree.nodes.len(), 3);
    assert_eq!(
        tree.nodes[1].mv,
        Some(MoveView::Play {
            color: ColorView::Black,
            point: PointView { x: 1, y: 1 }
        })
    );
    assert_eq!(
        tree.nodes[2].mv,
        Some(MoveView::Play {
            color: ColorView::White,
            point: PointView { x: 3, y: 3 }
        })
    );
    let mut saved = saved_document(&handle, directory.path()).await;
    saved.apply(document::Command::Step(i32::MAX)).unwrap();
    assert_eq!(saved.position_at(saved.current()).unwrap().moves.len(), 2);
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn helper_error_and_oversized_output_are_terminal_and_reaped() {
    for replay in [
        "error recognition failed\n".to_owned(),
        format!("{}\n", "x".repeat(9000)),
    ] {
        let directory = TempDir::new().unwrap();
        let program = helper(directory.path(), &replay);
        let (handle, mut output) = app(directory.path()).await;
        within(handle.start_board_sync(program, BoardSyncOptions::default()))
            .await
            .unwrap();
        let status = within(async {
            loop {
                if let AppEvent::BoardSyncStatusChanged(status) = output.recv().await.unwrap()
                    && status.phase == BoardSyncPhase::Failed
                {
                    break status;
                }
            }
        })
        .await;
        assert!(matches!(
            status.error,
            Some(BoardSyncError::Process { .. } | BoardSyncError::Protocol { .. })
        ));
        assert_eq!(
            std::fs::read_to_string(directory.path().join("readboard/exited")).unwrap(),
            "quit\n"
        );
        within(handle.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn helper_endsync_is_normal_shutdown_and_preserves_final_board() {
    let directory = TempDir::new().unwrap();
    let program = helper(directory.path(), &format!("{}endsync\n", two_moves()));
    let (handle, mut output) = app(directory.path()).await;
    within(handle.start_board_sync(program, BoardSyncOptions::default()))
        .await
        .unwrap();
    within(async {
        loop {
            if let AppEvent::BoardSyncStatusChanged(status) = output.recv().await.unwrap()
                && status.phase == BoardSyncPhase::Stopped
            {
                assert!(status.error.is_none());
                break;
            }
        }
    })
    .await;
    assert_eq!(
        std::fs::read_to_string(directory.path().join("readboard/exited")).unwrap(),
        "quit\n"
    );
    let mut document = saved_document(&handle, directory.path()).await;
    document.apply(document::Command::Step(i32::MAX)).unwrap();
    assert_eq!(
        document
            .position_at(document.current())
            .unwrap()
            .moves
            .len(),
        2
    );
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn conflicting_jump_rebuilds_after_two_matching_frames() {
    let directory = TempDir::new().unwrap();
    let jump = frame(&[(0, 1), (24, 4)], None);
    let replay = format!("{}{}{}", two_moves(), jump, jump);
    let program = helper(directory.path(), &replay);
    let (handle, mut output) = app(directory.path()).await;
    within(handle.start_board_sync(program, BoardSyncOptions::default()))
        .await
        .unwrap();
    let view = within(async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = output.recv().await.unwrap()
                && view.board.as_ref().is_some_and(|board| {
                    board.stones.len() == 25
                        && board.stones[0] == Some(ColorView::Black)
                        && board.stones[24] == Some(ColorView::White)
                })
            {
                return view;
            }
        }
    })
    .await;
    assert_eq!(
        view.board.unwrap().move_number,
        0,
        "unseen move history must not be invented"
    );
    let tree = within(handle.variation_tree()).await.unwrap();
    assert_eq!(tree.nodes.len(), 1);
    let saved = saved_document(&handle, directory.path()).await;
    let position = saved.position_at(saved.root()).unwrap();
    assert!(position.moves.is_empty());
    assert_eq!(position.initial_stones.len(), 2);
    assert_eq!(position.board.stone_at(go_core::Point { x: 1, y: 1 }), None);
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn stop_reaps_process_and_repeated_start_and_stop_are_idempotent() {
    let directory = TempDir::new().unwrap();
    let program = helper(directory.path(), &two_moves());
    let (handle, mut output) = app(directory.path()).await;
    within(handle.start_board_sync(program.clone(), BoardSyncOptions::default()))
        .await
        .unwrap();
    synced(&mut output, 2).await;
    let repeated = within(handle.start_board_sync(program, BoardSyncOptions::default()))
        .await
        .unwrap();
    assert_eq!(repeated.remote_move_number, Some(2));
    assert_eq!(
        within(handle.stop_board_sync()).await.unwrap().phase,
        BoardSyncPhase::Stopped
    );
    assert_eq!(
        within(handle.stop_board_sync()).await.unwrap().phase,
        BoardSyncPhase::Stopped
    );
    let resource = directory.path().join("readboard");
    assert_eq!(
        std::fs::read_to_string(resource.join("exited")).unwrap(),
        "quit\n"
    );
    let commands = std::fs::read_to_string(resource.join("commands.log")).unwrap();
    assert_eq!(
        commands.lines().filter(|line| *line == "version").count(),
        1
    );
    assert_eq!(commands.lines().filter(|line| *line == "quit").count(), 1);
    assert_eq!(
        within(handle.variation_tree()).await.unwrap().nodes.len(),
        3
    );
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn missing_readboard_is_typed_and_preserves_working_session() {
    let directory = TempDir::new().unwrap();
    let program = helper(directory.path(), &two_moves());
    let missing = directory.path().join("missing/readboard.exe");
    let (handle, mut output) = app(directory.path()).await;
    let error = within(handle.start_board_sync(missing.clone(), BoardSyncOptions::default()))
        .await
        .unwrap_err();
    assert!(
        matches!(error, AppError::BoardSync(BoardSyncError::MissingExecutable { path }) if path == missing)
    );
    assert_eq!(
        within(handle.board_sync_status()).await.unwrap().phase,
        BoardSyncPhase::Stopped
    );
    within(handle.start_board_sync(program, BoardSyncOptions::default()))
        .await
        .unwrap();
    synced(&mut output, 2).await;
    assert!(matches!(
        within(handle.start_board_sync(missing, BoardSyncOptions::default()))
            .await
            .unwrap_err(),
        AppError::BoardSync(BoardSyncError::MissingExecutable { .. })
    ));
    assert_eq!(
        within(handle.board_sync_status()).await.unwrap().phase,
        BoardSyncPhase::Syncing
    );
    assert_eq!(
        within(handle.variation_tree()).await.unwrap().nodes.len(),
        3
    );
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn dropping_last_handle_reaps_readboard_while_subscription_survives() {
    let directory = TempDir::new().unwrap();
    let program = helper(directory.path(), &two_moves());
    let (handle, mut output) = app(directory.path()).await;
    within(handle.start_board_sync(program, BoardSyncOptions::default()))
        .await
        .unwrap();
    synced(&mut output, 2).await;
    drop(handle);
    within(async { while output.recv().await.is_some() {} }).await;
    assert_eq!(
        std::fs::read_to_string(directory.path().join("readboard/exited")).unwrap(),
        "quit\n"
    );
}

#[tokio::test]
async fn malformed_frame_keeps_last_good_document_and_reports_protocol_error() {
    let directory = TempDir::new().unwrap();
    let replay = format!("{}re=0,5,0,0,0\nre=0,0,0,0,0\nend\n", two_moves());
    let program = helper(directory.path(), &replay);
    let (handle, mut output) = app(directory.path()).await;
    within(handle.start_board_sync(program, BoardSyncOptions::default()))
        .await
        .unwrap();
    within(async {
        loop {
            if let AppEvent::BoardSyncStatusChanged(status) = output.recv().await.unwrap()
                && matches!(status.error, Some(BoardSyncError::Protocol { .. }))
            {
                break;
            }
        }
    })
    .await;
    let tree = within(handle.variation_tree()).await.unwrap();
    assert_eq!(tree.nodes.len(), 3);
    let mut saved = saved_document(&handle, directory.path()).await;
    saved.apply(document::Command::Step(i32::MAX)).unwrap();
    let position = saved.position_at(saved.current()).unwrap();
    assert_eq!(position.moves.len(), 2);
    assert_eq!(
        position.board.stone_at(go_core::Point { x: 3, y: 3 }),
        Some(go_core::Color::White)
    );
    within(handle.shutdown()).await.unwrap();
}

fn engine_profile(directory: &Path) -> EngineProfile {
    let program = fake_katago::binary_path().unwrap();
    let model = directory.join("model.bin");
    let config = directory.join("analysis.cfg");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&config, "reportAnalysisWinratesAs = BLACK\n").unwrap();
    EngineProfile {
        canonical_model_name: None,
        program,
        model,
        config,
        extra_args: vec![
            "--visits-per-sec".into(),
            "100000".into(),
            "--request-log".into(),
            directory
                .join("requests.jsonl")
                .to_string_lossy()
                .into_owned(),
        ],
    }
}

fn analysis_queries(directory: &Path) -> Vec<Value> {
    std::fs::read_to_string(directory.join("requests.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|value| value.get("moves").is_some())
        .collect()
}

#[tokio::test]
async fn syncing_analyzes_latest_despite_saved_pause_and_stop_restores_pause() {
    let directory = TempDir::new().unwrap();
    let program = helper(directory.path(), &two_moves());
    let engine = engine_profile(directory.path());
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 10;
    config.analysis.live.report_interval = Duration::from_millis(10);
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    let (handle, mut output) = within(app_core::spawn_with_config(
        directory.path().join("settings"),
        config,
    ))
    .await
    .unwrap();
    within(handle.set_pondering(false)).await.unwrap();
    within(handle.configure_engine(Some(engine))).await.unwrap();
    within(async {
        loop {
            match output.recv().await.unwrap() {
                AppEvent::EngineStatusChanged(EngineStatus::Ready { .. }) => break,
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("engine failed: {reason}")
                }
                _ => {}
            }
        }
    })
    .await;
    within(handle.start_board_sync(program, BoardSyncOptions::default()))
        .await
        .unwrap();
    let view = synced(&mut output, 2).await;
    let analysis = within(async {
        loop {
            match output.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(value)
                    if value.node_id == view.tree.current && !value.is_during_search =>
                {
                    break value;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("engine failed: {reason}")
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(analysis.visits, 10);
    assert!(!within(handle.settings()).await.unwrap().pondering);
    let before_stop = analysis_queries(directory.path());
    assert!(before_stop.iter().any(|query| {
        query["moves"]
            .as_array()
            .is_some_and(|moves| moves.len() == 2)
    }));
    within(handle.stop_board_sync()).await.unwrap();
    assert!(!within(handle.settings()).await.unwrap().pondering);
    within(handle.step(-1)).await.unwrap();
    within(handle.shutdown()).await.unwrap();
    let after_stop = analysis_queries(directory.path());
    assert_eq!(
        after_stop.len(),
        before_stop.len(),
        "navigation after stop must remain paused"
    );
    let commands =
        std::fs::read_to_string(directory.path().join("readboard/commands.log")).unwrap();
    assert!(commands.contains("analysisState running\n"));
    assert!(commands.contains("analysisState paused\n"));
}
