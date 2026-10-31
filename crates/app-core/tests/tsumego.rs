#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AnalysisView, AppConfig, AppCore, AppError, AppEvent, AppSubscription, ColorView, DocumentView,
    EngineProfile, EngineStatus, TsumegoAttacker, TsumegoError, TsumegoKoThreat, TsumegoOptions,
    TsumegoToPlay,
};
use document::{Command, GameDocument};
use go_core::{Color, Move, Point};
use std::{future::Future, path::Path, time::Duration};

const SOURCE: &[u8] = b"(;SZ[19]KM[6.5]RU[japanese]PB[original black]C[root]AB[aa][ab][ba]AW[bb]PL[W];W[bc](;B[ac])(;B[cc]))";

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("operation exceeded safety deadline")
}

async fn document(output: &mut AppSubscription, expected: u64) -> DocumentView {
    within(async {
        loop {
            match output.recv().await.unwrap() {
                AppEvent::DocumentChanged { revision, view } => {
                    assert_eq!(revision, expected);
                    return view;
                }
                AppEvent::AnalysisUpdated(view) => panic!("analysis before document: {view:?}"),
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
                AppEvent::AnalysisUpdated(view) => {
                    assert_eq!((view.revision, view.node_id), (revision, node));
                    if !view.is_during_search {
                        return view;
                    }
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("{reason}")
                }
                _ => {}
            }
        }
    })
    .await
}

async fn open_source(handle: &AppCore, output: &mut AppSubscription) -> (u64, DocumentView) {
    within(handle.open_bytes(SOURCE.to_vec())).await.unwrap();
    let revision = within(handle.step(1)).await.unwrap();
    (revision, document(output, revision).await)
}

fn generated_original(options: TsumegoOptions) -> go_core::tsumego::TsumegoOutput {
    let collection = sgf::parse(SOURCE).unwrap();
    let mut original = GameDocument::from_sgf(&collection.games[0]).unwrap();
    original.apply(Command::Step(1)).unwrap();
    let position = original.position_at(original.current()).unwrap();
    go_core::tsumego::generate(
        position.board.size(),
        position.board.stones(),
        position.board.to_play(),
        options.into(),
    )
    .unwrap()
}

async fn saved(handle: &AppCore, directory: &Path, name: &str) -> Vec<u8> {
    let path = directory.join(name);
    within(handle.save(path.clone())).await.unwrap();
    std::fs::read(path).unwrap()
}

#[tokio::test]
async fn generation_flattens_to_setup_root_and_exit_restores_tree_focus_and_edit_history() {
    let directory = tempfile::tempdir().unwrap();
    let (handle, mut output) = within(app_core::spawn(directory.path())).await.unwrap();
    let (_, source) = open_source(&handle, &mut output).await;
    within(handle.apply(Command::SetComment("retained edit".into())))
        .await
        .unwrap();
    within(handle.play(Move::Play(Color::Black, Point { x: 3, y: 3 })))
        .await
        .unwrap();
    let source_revision = within(handle.undo()).await.unwrap();
    let original_view = document(&mut output, source_revision).await;
    assert_eq!(original_view.board, source.board);
    let original_tree = within(handle.variation_tree()).await.unwrap();
    let original_bytes = saved(&handle, directory.path(), "original.sgf").await;
    let expected = generated_original(TsumegoOptions::default());

    let revision = within(handle.generate_tsumego(TsumegoOptions::default()))
        .await
        .unwrap();
    let generated = document(&mut output, revision).await;
    let board = generated.board.as_ref().unwrap();
    assert_eq!(
        board.stones,
        expected
            .stones
            .into_iter()
            .map(|c| c.map(Into::into))
            .collect::<Vec<_>>()
    );
    assert_eq!(board.to_play, ColorView::Black);
    assert_eq!(board.move_number, 0);
    assert_eq!((board.captured_black, board.captured_white), (0, 0));
    assert_eq!(board.last_move, None);
    assert_eq!(generated.game_info.komi, expected.komi.unwrap());
    assert_eq!(generated.game_info.rules.as_deref(), Some("japanese"));
    assert_eq!(generated.game_info.black_name, None);
    let tree = within(handle.variation_tree()).await.unwrap();
    assert_eq!(tree.nodes.len(), 1);
    assert_eq!(tree.nodes[0].parent, None);
    assert_eq!(tree.nodes[0].mv, None);
    assert!(tree.revision > original_tree.revision);
    let status = within(handle.tsumego_status()).await.unwrap();
    assert_eq!(status.source_node, Some(original_view.tree.current));
    assert_eq!(status.revision, revision);
    assert!(status.active);
    assert_eq!(status.attacker, Some(ColorView::Black));
    let bytes = saved(&handle, directory.path(), "tsumego.sgf").await;
    let collection = sgf::parse(&bytes).unwrap();
    let round_trip = GameDocument::from_sgf(&collection.games[0]).unwrap();
    let position = round_trip.position_at(round_trip.root()).unwrap();
    assert!(position.moves.is_empty());
    assert!(position.analysis_moves.is_empty());
    assert_eq!(position.komi, expected.komi.unwrap());
    assert_eq!(
        position.initial_stones.len(),
        board.stones.iter().flatten().count()
    );

    within(handle.play(Move::Pass(Color::Black))).await.unwrap();
    let revision = within(handle.exit_tsumego()).await.unwrap();
    let restored = document(&mut output, revision).await;
    assert_eq!(restored.board, original_view.board);
    assert_eq!(restored.tree.current, original_view.tree.current);
    assert_eq!(restored.comment, original_view.comment);
    assert_eq!(restored.game_info, original_view.game_info);
    assert_eq!(
        within(handle.variation_tree()).await.unwrap().nodes,
        original_tree.nodes
    );
    assert_eq!(
        saved(&handle, directory.path(), "restored.sgf").await,
        original_bytes
    );
    assert!(!within(handle.tsumego_status()).await.unwrap().active);
    assert_eq!(within(handle.exit_tsumego()).await.unwrap(), revision);
    let revision = within(handle.redo()).await.unwrap();
    assert_eq!(
        document(&mut output, revision).await.board.unwrap().stones[60],
        Some(ColorView::Black)
    );
    within(handle.undo()).await.unwrap();
    let revision = within(handle.undo()).await.unwrap();
    assert_eq!(document(&mut output, revision).await.comment, None);
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn repeated_generation_uses_original_position_and_persists_all_four_options() {
    let directory = tempfile::tempdir().unwrap();
    let (handle, mut output) = within(app_core::spawn(directory.path())).await.unwrap();
    let (_, original) = open_source(&handle, &mut output).await;
    within(handle.generate_tsumego(TsumegoOptions::default()))
        .await
        .unwrap();
    within(handle.play(Move::Pass(Color::Black))).await.unwrap();
    let options = TsumegoOptions {
        to_play: TsumegoToPlay::White,
        attacker: TsumegoAttacker::White,
        ko_threat: TsumegoKoThreat::Defender,
        wall_distance: 3,
    };
    let expected = generated_original(options);
    let revision = within(handle.generate_tsumego(options)).await.unwrap();
    let board = document(&mut output, revision).await.board.unwrap();
    assert_eq!(board.move_number, 0);
    assert_eq!(board.to_play, ColorView::White);
    assert_eq!(
        board.stones,
        expected
            .stones
            .into_iter()
            .map(|c| c.map(Into::into))
            .collect::<Vec<_>>()
    );
    let status = within(handle.tsumego_status()).await.unwrap();
    assert_eq!(status.options, options);
    assert_eq!(status.source_node, Some(original.tree.current));
    assert_eq!(status.attacker, Some(ColorView::White));
    let inherited_options = TsumegoOptions {
        to_play: TsumegoToPlay::Inherit,
        ..options
    };
    let revision = within(handle.generate_tsumego(inherited_options))
        .await
        .unwrap();
    assert_eq!(
        document(&mut output, revision).await.board.unwrap().to_play,
        ColorView::Black
    );
    let revision = within(handle.exit_tsumego()).await.unwrap();
    assert_eq!(document(&mut output, revision).await.board, original.board);
    within(handle.shutdown()).await.unwrap();
    let (handle, _) = within(app_core::spawn(directory.path())).await.unwrap();
    assert_eq!(
        within(handle.settings()).await.unwrap().tsumego,
        inherited_options
    );
    let status = within(handle.tsumego_status()).await.unwrap();
    assert_eq!(status.options, inherited_options);
    assert!(!status.active);
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn errors_keep_document_session_options_and_revision_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let (handle, mut output) = within(app_core::spawn(directory.path())).await.unwrap();
    open_source(&handle, &mut output).await;
    within(handle.generate_tsumego(TsumegoOptions::default()))
        .await
        .unwrap();
    let before = within(handle.tsumego_status()).await.unwrap();
    let tree = within(handle.variation_tree()).await.unwrap();
    assert!(matches!(
        within(handle.generate_tsumego(TsumegoOptions {
            wall_distance: 0,
            ..Default::default()
        }))
        .await,
        Err(AppError::Tsumego(TsumegoError::InvalidDistance))
    ));
    assert!(matches!(
        within(handle.generate_tsumego(TsumegoOptions {
            wall_distance: u32::MAX,
            ..Default::default()
        }))
        .await,
        Err(AppError::Tsumego(TsumegoError::NoSpace))
    ));
    assert!(
        within(handle.open_bytes(b"not SGF".to_vec()))
            .await
            .is_err()
    );
    assert_eq!(within(handle.tsumego_status()).await.unwrap(), before);
    assert_eq!(within(handle.variation_tree()).await.unwrap(), tree);
    let path = directory.path().join("settings.json");
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(
        within(handle.generate_tsumego(TsumegoOptions {
            wall_distance: 2,
            ..Default::default()
        }))
        .await
        .is_err()
    );
    assert_eq!(within(handle.tsumego_status()).await.unwrap(), before);
    assert_eq!(within(handle.variation_tree()).await.unwrap(), tree);
    let revision = within(handle.open_bytes(b"(;SZ[19]AB[aa][sa][as][ss])".to_vec()))
        .await
        .unwrap();
    assert!(matches!(
        within(handle.generate_tsumego(TsumegoOptions::default())).await,
        Err(AppError::Tsumego(TsumegoError::NoSpace))
    ));
    assert_eq!(within(handle.exit_tsumego()).await.unwrap(), revision);
    assert!(!within(handle.tsumego_status()).await.unwrap().active);
    within(handle.open_bytes(b"(;B[aa];W[aa])".to_vec()))
        .await
        .unwrap();
    let revision = within(handle.step(1)).await.unwrap();
    assert!(matches!(
        within(handle.generate_tsumego(TsumegoOptions::default())).await,
        Err(AppError::Document(_))
    ));
    assert_eq!(
        within(handle.tsumego_status()).await.unwrap().revision,
        revision
    );
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn generated_and_restored_analysis_use_fresh_namespaces_and_exact_setup_queries() {
    let directory = tempfile::tempdir().unwrap();
    let model = directory.path().join("model.bin");
    let config_path = directory.path().join("analysis.cfg");
    let log = directory.path().join("requests.jsonl");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&config_path, "fake config").unwrap();
    let engine = EngineProfile {
        program: fake_katago::binary_path().unwrap(),
        model,
        config: config_path,
        extra_args: vec![
            "--visits-per-sec".into(),
            "100000".into(),
            "--request-log".into(),
            log.to_str().unwrap().into(),
        ],
        canonical_model_name: None,
    };
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 10;
    config.analysis.live.report_interval = Duration::from_millis(10);
    let (handle, mut output) = within(app_core::spawn_with_config(directory.path(), config))
        .await
        .unwrap();
    let (revision, original) = open_source(&handle, &mut output).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    final_analysis(&mut output, revision, original.tree.current).await;
    let revision = within(handle.generate_tsumego(TsumegoOptions::default()))
        .await
        .unwrap();
    let generated = document(&mut output, revision).await;
    final_analysis(&mut output, revision, generated.tree.root).await;
    let revision = within(handle.exit_tsumego()).await.unwrap();
    let restored = document(&mut output, revision).await;
    assert_eq!(restored.board, original.board);
    final_analysis(&mut output, revision, restored.tree.current).await;
    within(handle.shutdown()).await.unwrap();

    let queries: Vec<katago_protocol::AnalysisQuery> = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value.get("action").is_none())
        .map(|value| serde_json::from_value(value).unwrap())
        .collect();
    assert_eq!(queries.len(), 3);
    let query = &queries[1];
    assert!(query.moves.is_empty());
    assert_eq!(query.komi, Some(generated.game_info.komi));
    assert_eq!(query.initial_player, Some(katago_protocol::Player::Black));
    assert_eq!(
        query.rules,
        katago_protocol::Rules::Named("japanese".into())
    );
    // Omitting analyzeTurns requests the final turn, which is setup turn zero.
    assert_eq!(query.analyze_turns, None);
    let expected = generated_original(TsumegoOptions::default());
    let expected_stones: Vec<_> = expected
        .stones
        .iter()
        .enumerate()
        .filter_map(|(index, color)| {
            color.map(|color| {
                (
                    if color == Color::Black {
                        katago_protocol::Player::Black
                    } else {
                        katago_protocol::Player::White
                    },
                    Point {
                        x: (index % 19) as u8,
                        y: (index / 19) as u8,
                    }
                    .to_gtp(go_core::BoardSize::new(19, 19).unwrap())
                    .unwrap(),
                )
            })
        })
        .collect();
    assert_eq!(query.initial_stones, Some(expected_stones));
    assert_eq!(queries[0].moves, queries[2].moves);
    assert_eq!(queries[0].initial_stones, queries[2].initial_stones);
    assert_eq!(queries[0].komi, queries[2].komi);
    assert_eq!(queries[0].initial_player, queries[2].initial_player);
}
