#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{
    EngineProfile, WholeGameOptions,
    actor::tests::{actor, snapshot},
};
use serde_json::Value;
use std::{path::Path, time::Duration};

async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("operation exceeded safety deadline")
}

async fn logged(path: &Path, predicate: impl Fn(&Value) -> bool) -> Value {
    within(async {
        loop {
            if let Ok(text) = std::fs::read_to_string(path) {
                // Ignore the final incomplete line if a write is in progress.
                for line in text.lines() {
                    if let Ok(value) = serde_json::from_str::<Value>(line)
                        && predicate(&value)
                    {
                        return value;
                    }
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}

#[tokio::test]
async fn live_results_completing_review_terminate_the_still_pending_batch() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("requests.jsonl");
    let program = fake_katago::binary_path().unwrap();
    let (mut actor, _output) = actor();
    actor.directory = directory.path().to_owned();
    actor.settings.pondering = false;
    actor.settings.engine = Some(EngineProfile {
        canonical_model_name: None,
        model: program.clone(),
        config: program.clone(),
        program,
        extra_args: vec![
            "--visits-per-sec".into(),
            "1000".into(),
            "--request-log".into(),
            log.to_str().unwrap().into(),
        ],
    });
    actor.config.session.write_timeout = Duration::from_secs(60);
    actor.config.session.shutdown_timeout = Duration::from_secs(60);
    let parsed = sgf::parse(b"(;SZ[9];B[];W[])").unwrap();
    actor.document = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
    within(actor.start_engine()).await;
    let started = within(actor.review_command(ReviewInput::Start(WholeGameOptions {
        visits: 1_000_000,
        ..Default::default()
    })))
    .await
    .unwrap();
    assert_eq!((started.completed, started.total), (0, 3));
    let request = logged(&log, |query| query["maxVisits"] == 1_000_000).await;
    assert_eq!(request["analyzeTurns"].as_array().unwrap().len(), 3);

    // Keep the real subscription pending while deterministically supplying the
    // saved live results. Neither wall-clock search speed nor event ordering
    // decides whether the review has enough data to finish.
    for (node_id, position) in actor.review.line.clone() {
        let mut live = snapshot(&actor, 1_000_000, 0.7);
        live.node_id = node_id;
        live.key = position.analysis_key();
        actor.retain_analysis(&live);
    }
    let root = actor.stored_analysis(actor.document.root().0).unwrap();
    within(actor.review_signal(Some(Ok(vec![ReviewResult {
        node_id: root.node_id,
        key: root.key,
        response: root.response,
    }]))))
    .await;
    let progress = actor.review.progress.as_ref().unwrap();
    assert_eq!(progress.status, WholeGameStatus::Completed);
    assert_eq!((progress.completed, progress.total), (3, 3));
    assert!(actor.review.active.is_none());
    assert!(actor.review.batches.is_empty());
    logged(&log, |query| {
        query["action"] == "terminate" && query["terminateId"] == request["id"]
    })
    .await;
    within(actor.stop_engine()).await.unwrap();
}

fn capture_line(actor: &Actor) -> Vec<(NodeId, GamePosition)> {
    GraphView::from_document(&actor.document)
        .unwrap()
        .points
        .into_iter()
        .map(|point| {
            let id = NodeId(point.node_id);
            (id, actor.document.position_at(id).unwrap())
        })
        .collect()
}

fn retain_node(actor: &mut Actor, node_id: NodeId, position: &GamePosition, visits: u64) {
    let mut live = snapshot(actor, visits, 0.7);
    live.node_id = node_id;
    live.key = position.analysis_key();
    actor.retain_analysis(&live);
}

#[tokio::test]
async fn batch_boundary_completes_without_submitting_when_live_data_satisfies_every_node() {
    let (mut actor, _output) = actor();
    let parsed = sgf::parse(b"(;SZ[9];B[];W[])").unwrap();
    actor.document = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
    let line = capture_line(&actor);
    // Model a queued later batch whose nodes have become sufficiently deep
    // through live analysis since the task's original plan was constructed.
    actor.review.line = line.clone();
    actor.review.batches = ReviewPlan::new(line.clone(), 500)
        .unwrap()
        .into_batches()
        .into();
    actor.review.progress = Some(WholeGameProgress {
        source: WholeGameSource::Manual,
        completed: 1,
        total: line.len(),
        visits: 500,
        status: WholeGameStatus::Running,
    });
    for (node, position) in line {
        retain_node(&mut actor, node, &position, 500);
    }
    // No engine is installed: submitting the stale plan would report NoEngine.
    // Receiving the previous batch's end must instead recheck saved results.
    actor.review_signal(None).await;
    let progress = actor.review.progress.as_ref().unwrap();
    assert_eq!(progress.status, WholeGameStatus::Completed);
    assert_eq!((progress.completed, progress.total), (3, 3));
    assert!(actor.review.active.is_none());
    assert!(actor.review.batches.is_empty());
}

#[tokio::test]
async fn batch_boundary_omits_later_turns_that_live_analysis_already_completed() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("requests.jsonl");
    let program = fake_katago::binary_path().unwrap();
    let (mut actor, _output) = actor();
    actor.directory = directory.path().to_owned();
    actor.settings.pondering = false;
    actor.settings.engine = Some(EngineProfile {
        canonical_model_name: None,
        model: program.clone(),
        config: program.clone(),
        program,
        extra_args: vec![
            "--visits-per-sec".into(),
            "1000".into(),
            "--request-log".into(),
            log.to_str().unwrap().into(),
        ],
    });
    actor.config.session.write_timeout = Duration::from_secs(60);
    actor.config.session.shutdown_timeout = Duration::from_secs(60);
    let parsed = sgf::parse(b"(;SZ[9];B[];W[])").unwrap();
    actor.document = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
    within(actor.start_engine()).await;
    let line = capture_line(&actor);
    actor.review.line = line.clone();
    actor.review.batches = ReviewPlan::new(line.clone(), 1_000_000)
        .unwrap()
        .into_batches()
        .into();
    actor.review.progress = Some(WholeGameProgress {
        source: WholeGameSource::Manual,
        completed: 1,
        total: line.len(),
        visits: 1_000_000,
        status: WholeGameStatus::Running,
    });
    retain_node(&mut actor, line[0].0, &line[0].1, 1_000_000);
    retain_node(&mut actor, line[2].0, &line[2].1, 1_000_000);
    within(actor.review_signal(None)).await;
    let request = logged(&log, |query| query["maxVisits"] == 1_000_000).await;
    assert_eq!(request["analyzeTurns"], serde_json::json!([1]));
    let progress = actor.review.progress.as_ref().unwrap();
    assert_eq!(progress.status, WholeGameStatus::Running);
    assert_eq!((progress.completed, progress.total), (2, 3));
    within(actor.stop_review_query()).await.unwrap();
    logged(&log, |query| {
        query["action"] == "terminate" && query["terminateId"] == request["id"]
    })
    .await;
    within(actor.stop_engine()).await.unwrap();
}

#[test]
fn invalidation_clears_finished_task_line_and_prevents_stale_completed_status() {
    let (mut actor, _) = super::super::tests::actor();
    let saved = super::super::tests::snapshot(&actor, 1000, 0.7);
    actor.retain_analysis(&saved);
    actor.review.progress = Some(WholeGameProgress {
        source: WholeGameSource::Manual,
        completed: 1,
        total: 1,
        visits: 1000,
        status: WholeGameStatus::Completed,
    });
    actor.invalidate_review();
    assert!(actor.review.results.is_empty());
    assert!(actor.review.line.is_empty());
    assert_eq!(
        actor.review.progress.unwrap().status,
        WholeGameStatus::Cancelled
    );
}

#[test]
fn scopes_capture_move_endpoints_and_depth_first_order_without_navigating() {
    let (mut actor, _) = actor();
    let parsed = sgf::parse(b"(;SZ[9]C[root];B[]C[one];C[empty](;W[]C[two];B[]C[three])(;W[aa]C[branch](;B[bb]C[child])(;B[cc]C[sibling])))").unwrap();
    actor.document = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
    let initial = actor.document.current();
    let names = |scope| {
        actor
            .review_positions(scope)
            .unwrap()
            .into_iter()
            .map(|(id, _)| {
                actor
                    .document
                    .node(id)
                    .unwrap()
                    .comment()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(WholeGameScope::AllBranches),
        [
            "root", "one", "empty", "two", "three", "branch", "child", "sibling"
        ]
    );
    assert_eq!(
        names(WholeGameScope::Line),
        ["root", "one", "empty", "two", "three"]
    );
    assert_eq!(
        names(WholeGameScope::Range {
            first_move: None,
            last_move: None
        }),
        ["root", "one", "empty", "two", "three"]
    );
    assert_eq!(
        names(WholeGameScope::Range {
            first_move: Some(2),
            last_move: None
        }),
        ["empty", "two", "three"]
    );
    assert_eq!(
        names(WholeGameScope::Range {
            first_move: None,
            last_move: Some(1)
        }),
        ["root", "one", "empty"]
    );
    assert_eq!(
        names(WholeGameScope::Range {
            first_move: Some(2),
            last_move: Some(2)
        }),
        ["empty", "two"]
    );
    for (first_move, last_move) in [
        (Some(0), None),
        (None, Some(0)),
        (Some(4), None),
        (None, Some(4)),
        (Some(3), Some(2)),
    ] {
        assert!(matches!(
            actor.review_positions(WholeGameScope::Range {
                first_move,
                last_move
            }),
            Err(AppError::WholeGame(WholeGameError::InvalidRange { .. }))
        ));
    }
    assert_eq!(actor.document.current(), initial);
}

#[test]
fn empty_game_range_is_invalid_but_all_branches_includes_root() {
    let (actor, _) = actor();
    assert!(matches!(
        actor.review_positions(WholeGameScope::Range {
            first_move: None,
            last_move: None
        }),
        Err(AppError::WholeGame(WholeGameError::InvalidRange {
            last_available: 0,
            ..
        }))
    ));
    assert_eq!(
        actor
            .review_positions(WholeGameScope::AllBranches)
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn forced_review_resume_only_skips_finals_from_this_run() {
    let directory = tempfile::tempdir().unwrap();
    let program = fake_katago::binary_path().unwrap();
    let (mut actor, _) = actor();
    actor.directory = directory.path().to_owned();
    actor.settings.pondering = false;
    actor.settings.cache_enabled = false;
    actor.settings.engine = Some(EngineProfile {
        canonical_model_name: None,
        model: program.clone(),
        config: program.clone(),
        program,
        extra_args: Vec::new(),
    });
    let parsed = sgf::parse(b"(;SZ[9];B[];W[])").unwrap();
    actor.document = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
    let line = capture_line(&actor);
    actor.start_engine().await;
    for (node, position) in &line {
        retain_node(&mut actor, *node, position, 1_000_000);
    }
    let options = WholeGameOptions {
        visits: 1_000_000,
        force: true,
        ..Default::default()
    };
    let started = actor
        .review_command(ReviewInput::Start(options))
        .await
        .unwrap();
    assert_eq!(started.completed, 0);
    // Inject the already validated Final while the real fake search is pending.
    // Completion is driven by events, never by assumed elapsed process time.
    let root = actor.stored_analysis(actor.document.root().0).unwrap();
    actor
        .accept_review_results(vec![ReviewResult {
            node_id: root.node_id,
            key: root.key,
            response: root.response,
        }])
        .unwrap();
    assert_eq!(actor.review.progress.as_ref().unwrap().completed, 1);
    actor.pause_review().await.unwrap();
    let resumed = actor.resume_review().await.unwrap();
    assert_eq!((resumed.completed, resumed.total), (1, 3));
    assert_eq!(resumed.status, WholeGameStatus::Running);
    assert!(actor.review.active.is_some());
    actor.stop_engine().await.unwrap();
}
