#![allow(clippy::unwrap_used, clippy::expect_used)]

use analysis::{AnalysisConfig, Focus, LiveSettings, SnapshotSource, review::ReviewPlan};
use document::{Command, GameDocument, GamePosition, NodeId};
use engine_session::{EngineHandle, SessionConfig, TransportSpec};
use go_core::Move;
use std::{collections::BTreeMap, ffi::OsString, future::Future, time::Duration};
use tokio::time::timeout;

async fn within<T>(future: impl Future<Output = T>) -> T {
    timeout(Duration::from_secs(5), future)
        .await
        .expect("event safety deadline")
}

fn engine(extra: &[&str]) -> EngineHandle {
    let mut args: Vec<OsString> = [
        "analysis",
        "-config",
        "unused",
        "-model",
        "unused",
        "--visits-per-sec",
        "10000",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    args.extend(extra.iter().map(OsString::from));
    engine_session::spawn(
        TransportSpec::LocalProcess {
            program: fake_katago::binary_path().unwrap(),
            args,
            cwd: None,
            env: BTreeMap::new(),
        },
        SessionConfig::default(),
    )
}

fn line(plies: usize) -> Vec<(NodeId, GamePosition)> {
    let mut document = GameDocument::new();
    let mut line = vec![(
        document.root(),
        document.position_at(document.root()).unwrap(),
    )];
    for _ in 0..plies {
        let position = document.position_at(document.current()).unwrap();
        document
            .apply(Command::Play(Move::Pass(position.board.to_play())))
            .unwrap();
        line.push((
            document.current(),
            document.position_at(document.current()).unwrap(),
        ));
    }
    line
}

fn focus(node: &(NodeId, GamePosition), revision: u64) -> Focus {
    Focus {
        node_id: node.0,
        revision,
        position: node.1.clone(),
        key: node.1.analysis_key(),
    }
}

#[tokio::test]
async fn whole_game_finals_keep_full_payload_and_live_updates_continue() {
    // The same override as the app profile is honored by fake-katago.
    let engine = engine(&["-override-config", "numAnalysisThreads=2"]);
    let positions = line(12);
    let (live, mut output) = analysis::spawn(
        engine.clone(),
        AnalysisConfig {
            live: LiveSettings {
                report_interval: Duration::from_millis(10),
                ..LiveSettings::default()
            },
            ..AnalysisConfig::default()
        },
    )
    .unwrap();
    within(live.focus(focus(&positions[0], 1))).await.unwrap();
    let first = within(output.recv()).await.unwrap();
    let mut batch = within(
        ReviewPlan::new(positions.clone(), 500)
            .unwrap()
            .into_batches()
            .remove(0)
            .submit(&engine),
    )
    .await
    .unwrap();
    let (results, live_updates) = within(async {
        let mut results = BTreeMap::new();
        let mut updates = 0;
        loop {
            tokio::select! {
                value = batch.recv() => match value {
                    Some(Ok(values)) => for value in values { results.insert(value.node_id, value); },
                    Some(Err(error)) => panic!("review failed: {error}"),
                    None => return (results, updates),
                },
                value = output.recv() => {
                    let value = value.unwrap();
                    assert_eq!(value.node_id, positions[0].0);
                    assert!(value.response.root_info.visits > first.response.root_info.visits);
                    updates += 1;
                }
            }
        }
    }).await;
    assert!(live_updates >= 2);
    assert_eq!(results.len(), positions.len());
    for (id, position) in positions {
        let result = &results[&id];
        assert_eq!(result.key, position.analysis_key());
        assert_eq!(result.response.root_info.visits, 500);
        assert!(!result.response.is_during_search);
        assert!(!result.response.move_infos.is_empty());
        assert!(!result.response.move_infos[0].pv.is_empty());
        assert_eq!(result.response.ownership.as_ref().unwrap().len(), 19 * 19);
    }
    within(live.shutdown()).await.unwrap();
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn cancelling_batch_frees_single_search_slot_and_discards_late_results() {
    let engine = engine(&["--threads", "1"]);
    let mut batch = within(
        ReviewPlan::new(line(4), 1_000_000)
            .unwrap()
            .into_batches()
            .remove(0)
            .submit(&engine),
    )
    .await
    .unwrap();
    within(batch.cancel(&engine)).await.unwrap();
    assert!(within(batch.recv()).await.is_none());
    // This can only finish if the prior million-visit query was terminated.
    let mut next = within(
        ReviewPlan::new(line(0), 500)
            .unwrap()
            .into_batches()
            .remove(0)
            .submit(&engine),
    )
    .await
    .unwrap();
    assert_eq!(
        within(next.recv()).await.unwrap().unwrap()[0]
            .response
            .root_info
            .visits,
        500
    );
    assert!(within(next.recv()).await.is_none());
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn final_below_requested_depth_is_an_explicit_error_not_completed_work() {
    let engine = engine(&[]);
    let mut batch = ReviewPlan::new(line(1), 500)
        .unwrap()
        .into_batches()
        .remove(0);
    // Simulate an engine-side limit ending a search before the review target.
    batch.request.max_visits = Some(100);
    let mut run = within(batch.submit(&engine)).await.unwrap();
    assert!(matches!(
        within(run.recv()).await,
        Some(Err(analysis::AnalysisError::Review(reason))) if reason.contains("below target")
    ));
    within(run.cancel(&engine)).await.unwrap();
    assert!(within(run.recv()).await.is_none());
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn document_result_is_displayed_after_lru_eviction_before_deeper_live_search() {
    let engine = engine(&[]);
    let positions = line(1);
    let mut batch = within(
        ReviewPlan::new(positions.clone(), 500)
            .unwrap()
            .into_batches()
            .remove(0)
            .submit(&engine),
    )
    .await
    .unwrap();
    let mut retained = BTreeMap::new();
    while let Some(value) = within(batch.recv()).await {
        for value in value.unwrap() {
            retained.insert(value.node_id, value.response);
        }
    }
    let (handle, mut output) = analysis::spawn(
        engine.clone(),
        AnalysisConfig {
            pondering: false,
            cache_capacity: 1,
            live: LiveSettings {
                max_visits: 1000,
                ..LiveSettings::default()
            },
            ..AnalysisConfig::default()
        },
    )
    .unwrap();
    for (index, node) in positions.iter().enumerate() {
        within(handle.focus_with_cached(
            focus(node, index as u64 + 1),
            Some(retained[&node.0].clone()),
        ))
        .await
        .unwrap();
        let value = within(output.recv()).await.unwrap();
        assert_eq!(value.source, SnapshotSource::Cache);
        assert_eq!(value.response.root_info.visits, 500);
    }
    let root = &positions[0];
    within(handle.focus_with_cached(focus(root, 3), Some(retained[&root.0].clone())))
        .await
        .unwrap();
    within(handle.set_pondering(true)).await.unwrap();
    let cached = within(output.recv()).await.unwrap();
    assert_eq!(cached.source, SnapshotSource::Cache);
    assert_eq!(cached.response, retained[&root.0]);
    let result = within(async {
        loop {
            let value = output.recv().await.unwrap();
            assert!(value.response.root_info.visits >= 500);
            if !value.response.is_during_search {
                return value;
            }
        }
    })
    .await;
    assert_eq!(result.source, SnapshotSource::Live);
    assert_eq!(result.response.root_info.visits, 1000);
    within(handle.shutdown()).await.unwrap();
    within(engine.shutdown()).await.unwrap();
}
