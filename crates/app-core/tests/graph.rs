#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AnalysisSummary, AnalysisView, AppConfig, AppCore, AppEvent, AppSubscription, DocumentView,
    EngineProfile, EngineStatus,
};
use std::{future::Future, path::Path, time::Duration};
use tempfile::TempDir;
use tokio::time::timeout;

async fn within<T>(future: impl Future<Output = T>) -> T {
    timeout(Duration::from_secs(5), future)
        .await
        .expect("operation exceeded safety deadline")
}

fn profile(directory: &Path) -> EngineProfile {
    let model = directory.join("model.bin");
    let config = directory.join("analysis.cfg");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&config, "numAnalysisThreads = 1\n").unwrap();
    EngineProfile {
        canonical_model_name: None,
        program: fake_katago::binary_path().unwrap(),
        model,
        config,
        extra_args: vec!["--visits-per-sec".into(), "100000".into()],
    }
}

async fn app(directory: &Path) -> (AppCore, AppSubscription) {
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 100;
    config.analysis.live.report_interval = Duration::from_millis(10);
    // These are not timeout tests; avoid introducing faults under CPU load.
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    within(app_core::spawn_with_config(directory, config))
        .await
        .unwrap()
}

async fn document(output: &mut AppSubscription, revision: u64) -> DocumentView {
    within(async {
        loop {
            match output.recv().await.expect("app event stream closed") {
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

async fn final_analysis(output: &mut AppSubscription, revision: u64, node: u32) -> AnalysisView {
    within(async {
        let mut best_visits = 0;
        loop {
            match output.recv().await.expect("app event stream closed") {
                AppEvent::AnalysisUpdated(value) => {
                    assert_eq!((value.revision, value.node_id), (revision, node));
                    assert!(value.summary.visits >= best_visits);
                    assert!(value.summary.visits >= value.visits);
                    best_visits = value.summary.visits;
                    if !value.is_during_search {
                        assert_eq!(value.visits, 100);
                        assert_eq!(
                            value.summary,
                            AnalysisSummary {
                                winrate: value.winrate,
                                score_lead: value.score_lead,
                                visits: value.visits,
                            }
                        );
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

fn summary(value: &AnalysisView) -> AnalysisSummary {
    value.summary
}

fn line(view: &DocumentView) -> Vec<u32> {
    view.graph
        .points
        .iter()
        .map(|point| point.node_id)
        .collect()
}

#[tokio::test]
async fn corpus_graph_retains_best_forwarded_summaries_and_null_for_unvisited_nodes() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path());
    let (handle, mut output) = app(directory.path()).await;
    let revision =
        within(handle.open_bytes(include_bytes!("../../../fixtures/sgf/basic-19.sgf").to_vec()))
            .await
            .unwrap();
    let initial = document(&mut output, revision).await;
    assert_eq!(line(&initial), initial.tree.main_line);
    assert_eq!(initial.graph.current_index, 0);
    assert!(
        initial
            .graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );

    within(handle.configure_engine(Some(engine))).await.unwrap();
    let mut known = vec![final_analysis(&mut output, revision, initial.tree.root).await];
    for index in 1..=3 {
        let revision = within(handle.step(1)).await.unwrap();
        let view = document(&mut output, revision).await;
        assert_eq!(view.graph.current_index, index);
        assert_eq!(view.graph.points[index].node_id, view.tree.current);
        for value in &known {
            let point = view
                .graph
                .points
                .iter()
                .find(|point| point.node_id == value.node_id)
                .unwrap();
            assert_eq!(point.summary, Some(summary(value)));
        }
        known.push(final_analysis(&mut output, revision, view.tree.current).await);
    }

    let revision = within(handle.go_to(initial.tree.root)).await.unwrap();
    let returned = document(&mut output, revision).await;
    assert_eq!(returned.graph.current_index, 0);
    assert_eq!(line(&returned), line(&initial));
    for (index, point) in returned.graph.points.iter().enumerate() {
        assert_eq!(point.move_number, index);
        assert_eq!(point.summary, known.get(index).map(summary));
    }
    // Cached and live results carry the best graph summary independently of
    // their current search depth.
    // The application's latest-event mailbox may coalesce those deliveries.
    let revisited = final_analysis(&mut output, revision, initial.tree.root).await;
    assert_eq!(summary(&revisited), summary(&known[0]));
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn graph_follows_current_variation_then_its_first_children() {
    let directory = TempDir::new().unwrap();
    let (handle, mut output) = app(directory.path()).await;
    let source = b"(;SZ[9];B[ee](;W[cc];B[dd])(;W[gg];C[empty](;B[ff];W[])(;B[hh])))";
    let parsed = sgf::parse(source).unwrap();
    let reference = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
    let root = reference.root();
    let fork = reference.node(root).unwrap().children()[0];
    let alternate = reference.node(fork).unwrap().children()[1];
    let empty = reference.node(alternate).unwrap().children()[0];
    let first = reference.node(empty).unwrap().children()[0];
    let tail = reference.node(first).unwrap().children()[0];
    let second = reference.node(empty).unwrap().children()[1];
    let revision = within(handle.open_bytes(source.to_vec())).await.unwrap();
    let initial = document(&mut output, revision).await;
    assert_eq!(line(&initial), initial.tree.main_line);

    let revision = within(handle.go_to(alternate.0)).await.unwrap();
    let view = document(&mut output, revision).await;
    assert_eq!(
        line(&view),
        vec![root.0, fork.0, alternate.0, empty.0, first.0, tail.0]
    );
    assert_eq!(view.graph.current_index, 2);
    assert_eq!(view.tree.main_line, initial.tree.main_line);

    let revision = within(handle.go_to(second.0)).await.unwrap();
    let view = document(&mut output, revision).await;
    assert_eq!(
        line(&view),
        vec![root.0, fork.0, alternate.0, empty.0, second.0]
    );
    assert_eq!(view.graph.current_index, 4);
    assert_eq!(view.graph.points[4].node_id, view.tree.current);

    let revision = within(handle.go_to(root.0)).await.unwrap();
    let returned = document(&mut output, revision).await;
    assert_eq!(line(&returned), line(&initial));
    assert_eq!(returned.graph.current_index, 0);
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn opening_another_document_clears_summaries_even_when_node_ids_are_reused() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path());
    let (handle, mut output) = app(directory.path()).await;
    let revision =
        within(handle.open_bytes(include_bytes!("../../../fixtures/sgf/basic-19.sgf").to_vec()))
            .await
            .unwrap();
    let initial = document(&mut output, revision).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    let root_analysis = final_analysis(&mut output, revision, initial.tree.root).await;
    within(handle.set_pondering(false)).await.unwrap();

    // A rejected open retains both the document and its analysis summaries.
    assert!(
        within(handle.open_bytes(b"not SGF".to_vec()))
            .await
            .is_err()
    );
    let revision = within(handle.apply(document::Command::SetComment("retained".into())))
        .await
        .unwrap();
    let retained = document(&mut output, revision).await;
    assert_eq!(
        retained.graph.points[0].summary,
        Some(summary(&root_analysis))
    );

    let revision =
        within(handle.open_bytes(include_bytes!("../../../fixtures/sgf/board-9.sgf").to_vec()))
            .await
            .unwrap();
    let replacement = document(&mut output, revision).await;
    assert_eq!(replacement.tree.root, initial.tree.root);
    assert_eq!(replacement.board.as_ref().unwrap().width, 9);
    assert!(
        replacement
            .graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );

    let revision = within(handle.step(1)).await.unwrap();
    let next = document(&mut output, revision).await;
    assert!(
        next.graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn graph_move_numbers_match_board_through_root_moves_passes_and_setup_nodes() {
    let directory = TempDir::new().unwrap();
    let (handle, mut output) = app(directory.path()).await;
    let source = b"(;SZ[9]AB[aa]PL[W]W[];C[empty];B[];AB[bb]PL[B];B[cc];C[end])";
    let revision = within(handle.open_bytes(source.to_vec())).await.unwrap();
    let initial = document(&mut output, revision).await;
    let expected = [1, 1, 2, 2, 3, 3];
    assert_eq!(
        initial
            .graph
            .points
            .iter()
            .map(|point| point.move_number)
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(initial.board.as_ref().unwrap().move_number, expected[0]);
    for (index, expected_move_number) in expected.into_iter().enumerate().skip(1) {
        let revision = within(handle.step(1)).await.unwrap();
        let view = document(&mut output, revision).await;
        let point = &view.graph.points[view.graph.current_index];
        assert_eq!(view.graph.current_index, index);
        assert_eq!(point.node_id, view.tree.current);
        assert_eq!(point.move_number, expected_move_number);
        assert_eq!(point.move_number, view.board.as_ref().unwrap().move_number);
    }
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn engine_fingerprint_changes_refresh_graph_without_changing_document_revision() {
    let directory = TempDir::new().unwrap();
    let mut engine = profile(directory.path());
    let (handle, mut output) = app(directory.path()).await;
    let revision =
        within(handle.open_bytes(include_bytes!("../../../fixtures/sgf/basic-19.sgf").to_vec()))
            .await
            .unwrap();
    let initial = document(&mut output, revision).await;
    within(handle.configure_engine(Some(engine.clone())))
        .await
        .unwrap();
    let root = final_analysis(&mut output, revision, initial.tree.root).await;
    let revision = within(handle.step(1)).await.unwrap();
    let first = document(&mut output, revision).await;
    let first_analysis = final_analysis(&mut output, revision, first.tree.current).await;
    within(handle.set_pondering(false)).await.unwrap();

    // Reapplying identical paths, arguments and metadata preserves all nodes.
    within(handle.configure_engine(Some(engine.clone())))
        .await
        .unwrap();
    let revision = within(handle.apply(document::Command::SetComment("same profile".into())))
        .await
        .unwrap();
    let retained = document(&mut output, revision).await;
    assert_eq!(retained.graph.points[0].summary, Some(root.summary));
    assert_eq!(
        retained.graph.points[1].summary,
        Some(first_analysis.summary)
    );

    engine.extra_args.extend(["--threads".into(), "2".into()]);
    within(handle.configure_engine(Some(engine.clone())))
        .await
        .unwrap();
    let changed_args = document(&mut output, revision).await;
    assert_eq!(changed_args.tree, retained.tree);
    assert_eq!(changed_args.board, retained.board);
    assert_eq!(changed_args.comment, retained.comment);
    assert_eq!(line(&changed_args), line(&retained));
    assert!(
        changed_args
            .graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );

    within(handle.set_pondering(true)).await.unwrap();
    let new_analysis = final_analysis(&mut output, revision, first.tree.current).await;
    let revision = within(handle.apply(document::Command::SetComment("new engine".into())))
        .await
        .unwrap();
    let refreshed = document(&mut output, revision).await;
    assert_eq!(refreshed.graph.points[0].summary, None);
    assert_eq!(
        refreshed.graph.points[1].summary,
        Some(new_analysis.summary)
    );
    within(handle.set_pondering(false)).await.unwrap();

    // A size change guarantees a distinct fingerprint without clock delays.
    // The path and profile values stay identical, exercising metadata checks.
    std::fs::write(&engine.model, "replacement model with a different size").unwrap();
    within(handle.configure_engine(Some(engine))).await.unwrap();
    let changed_model = document(&mut output, revision).await;
    assert_eq!(changed_model.tree, refreshed.tree);
    assert_eq!(changed_model.board, refreshed.board);
    assert_eq!(changed_model.comment, refreshed.comment);
    assert!(
        changed_model
            .graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );

    within(handle.set_pondering(true)).await.unwrap();
    let replacement_analysis = final_analysis(&mut output, revision, first.tree.current).await;
    within(handle.set_pondering(false)).await.unwrap();
    let revision = within(handle.apply(document::Command::SetComment("replacement model".into())))
        .await
        .unwrap();
    let final_view = document(&mut output, revision).await;
    assert_eq!(final_view.graph.points[0].summary, None);
    assert_eq!(
        final_view.graph.points[1].summary,
        Some(replacement_analysis.summary)
    );
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn disabling_or_selecting_an_unavailable_engine_clears_graph_summaries() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path());
    let (handle, mut output) = app(directory.path()).await;
    let revision =
        within(handle.open_bytes(include_bytes!("../../../fixtures/sgf/basic-19.sgf").to_vec()))
            .await
            .unwrap();
    let initial = document(&mut output, revision).await;
    within(handle.configure_engine(Some(engine.clone())))
        .await
        .unwrap();
    final_analysis(&mut output, revision, initial.tree.root).await;
    let revision = within(handle.step(1)).await.unwrap();
    let first = document(&mut output, revision).await;
    assert!(first.graph.points[0].summary.is_some());
    final_analysis(&mut output, revision, first.tree.current).await;

    within(handle.configure_engine(None)).await.unwrap();
    let disabled = document(&mut output, revision).await;
    assert_eq!(disabled.tree, first.tree);
    assert_eq!(disabled.board, first.board);
    assert!(
        disabled
            .graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );

    within(handle.configure_engine(Some(engine.clone())))
        .await
        .unwrap();
    final_analysis(&mut output, revision, first.tree.current).await;
    let mut missing_model = engine;
    missing_model.model = directory.path().join("missing-model.bin");
    within(handle.configure_engine(Some(missing_model)))
        .await
        .unwrap();
    let unavailable = document(&mut output, revision).await;
    assert_eq!(unavailable.tree, first.tree);
    assert_eq!(unavailable.board, first.board);
    assert!(
        unavailable
            .graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );
    within(async {
        loop {
            match output.recv().await.expect("app event stream closed") {
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    assert!(reason.contains("missing-model.bin"), "{reason}");
                    break;
                }
                AppEvent::AnalysisUpdated(value) => {
                    panic!("unavailable engine delivered analysis: {value:?}")
                }
                _ => {}
            }
        }
    })
    .await;
    within(handle.shutdown()).await.unwrap();
}
