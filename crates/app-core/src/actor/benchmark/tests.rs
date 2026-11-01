#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::{
    EngineProfile, WholeGameOptions,
    actor::tests::{actor, snapshot},
};
use std::time::Duration;

async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("deadline")
}

#[tokio::test]
async fn benchmark_resume_preserves_completed_review_nodes_and_original_pause() {
    let dir = tempfile::tempdir().unwrap();
    let program = fake_katago::binary_path().unwrap();
    let log = dir.path().join("requests");
    let (mut actor, _events) = actor();
    actor.directory = dir.path().to_owned();
    actor.settings.pondering = false;
    actor.settings.engine = Some(EngineProfile {
        program: program.clone(),
        model: program.clone(),
        config: program,
        extra_args: vec![
            "--request-log".into(),
            log.to_str().unwrap().into(),
            "--benchmark-gate-file".into(),
            dir.path().join("gate").to_str().unwrap().into(),
        ],
        canonical_model_name: None,
    });
    within(actor.apply(super::super::Input::OpenBytes(
        b"(;SZ[9];B[aa];W[bb])".to_vec(),
    )))
    .await
    .unwrap();
    within(actor.start_engine()).await;
    let mut completed = snapshot(&actor, 1_000_000, 0.7);
    completed.node_id = actor.document.root();
    completed.key = actor
        .document
        .position_at(completed.node_id)
        .unwrap()
        .analysis_key();
    actor.retain_analysis(&completed);
    within(actor.review_command(ReviewInput::Start(WholeGameOptions { visits: 1_000_000 })))
        .await
        .unwrap();
    assert_eq!(actor.review.progress.as_ref().unwrap().completed, 1);
    within(actor.begin_benchmark()).await.unwrap();
    assert!(actor.running.is_none());
    assert_eq!(
        actor.review.progress.as_ref().unwrap().status,
        WholeGameStatus::Paused
    );
    within(actor.benchmark_command(Request::Cancel))
        .await
        .unwrap();
    assert!(!actor.pondering());
    let progress = actor.review.progress.as_ref().unwrap();
    assert_eq!(
        (progress.completed, progress.status.clone()),
        (1, WholeGameStatus::Running)
    );
    assert_eq!(
        actor
            .stored_analysis(completed.node_id.0)
            .unwrap()
            .response
            .root_info
            .visits,
        1_000_000
    );
    // A review that was already paused must remain paused across another run.
    within(actor.review_command(ReviewInput::Pause))
        .await
        .unwrap();
    within(actor.begin_benchmark()).await.unwrap();
    within(actor.benchmark_command(Request::Cancel))
        .await
        .unwrap();
    assert_eq!(
        actor.review.progress.as_ref().unwrap().status,
        WholeGameStatus::Paused
    );
    within(actor.stop_engine()).await.unwrap();
    let requests = std::fs::read_to_string(log).unwrap();
    for line in requests.lines() {
        let request: serde_json::Value = serde_json::from_str(line).unwrap();
        if request["maxVisits"] == 1_000_000 {
            assert!(
                !request["analyzeTurns"]
                    .as_array()
                    .unwrap()
                    .contains(&serde_json::json!(0))
            );
        }
    }
}
