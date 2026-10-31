#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{AppSubscription, delivery};
use serde_json::json;
use std::time::Duration;

async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("operation exceeded safety deadline")
}

pub(super) fn actor() -> (Actor, AppSubscription) {
    let document = GameDocument::new();
    let position = document.position_at(document.root()).unwrap();
    let (output, subscription) = delivery::channel();
    let focus = ViewFocus {
        revision: 0,
        node_id: 0,
        key: position.analysis_key(),
        size: position.board.size(),
        to_play: position.board.to_play(),
    };
    (
        Actor {
            document,
            tsumego: None,
            tree: VariationTreeView::default(),
            summaries: HashMap::new(),
            review: Default::default(),
            strength: None,
            summary_fingerprint: None,
            revision: 0,
            directory: PathBuf::new(),
            settings: Settings::default(),
            config: AppConfig::default(),
            running: None,
            trial: None,
            auto_setup: Default::default(),
            board_sync: Default::default(),
            focus: Some(focus),
            output,
        },
        subscription,
    )
}

pub(super) fn snapshot(actor: &Actor, visits: u64, winrate: f64) -> AnalysisSnapshot {
    AnalysisSnapshot {
        revision: actor.revision,
        node_id: actor.document.current(),
        key: actor.focus.as_ref().unwrap().key,
        response: serde_json::from_value(json!({
            "id": "graph-unit", "turnNumber": 0, "isDuringSearch": false,
            "rootInfo": {
                "visits": visits, "winrate": winrate, "scoreLead": -3.5,
                "utility": 0.0, "currentPlayer": "B"
            },
            "moveInfos": [{
                "move": "D4", "visits": visits, "winrate": winrate,
                "scoreLead": -3.5, "prior": 0.2, "order": 0, "pv": ["D4"]
            }]
        }))
        .unwrap(),
        source: analysis::SnapshotSource::Live,
        visits_per_second: None,
    }
}

async fn event(output: &mut AppSubscription) -> Option<AppEvent> {
    within(output.recv()).await
}

/// Drive the same signal path as run(), with real coordinator/session actors
/// and the fake process. Commands are acknowledged before looking for results.
async fn final_analysis(actor: &mut Actor, output: &mut AppSubscription) -> AnalysisView {
    within(async {
        loop {
            tokio::select! {
                signal = next_signal(&mut actor.running) => actor.signal(signal),
                event = output.recv() => match event.expect("event stream closed") {
                    AppEvent::AnalysisUpdated(view) => {
                        assert_eq!(view.node_id, actor.document.current().0);
                        assert_eq!(view.revision, actor.revision);
                        if !view.is_during_search { return view; }
                    }
                    AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                        panic!("fake engine failed: {reason}");
                    }
                    _ => {}
                }
            }
        }
    })
    .await
}

#[tokio::test]
async fn lru_eviction_restores_persistent_analysis_with_the_previous_best_summary() {
    let directory = tempfile::tempdir().unwrap();
    let program = fake_katago::binary_path().unwrap();
    let profile = EngineProfile {
        canonical_model_name: None,
        model: program.clone(),
        config: program.clone(),
        program,
        extra_args: vec!["--visits-per-sec".into(), "100000".into()],
    };
    let (mut actor, mut output) = actor();
    actor.directory = directory.path().to_owned();
    actor.config.analysis.cache_capacity = 1;
    actor.config.analysis.live.max_visits = 100;
    // Only finals are needed. Do not depend on catching a short-lived partial.
    actor.config.analysis.live.report_interval = Duration::from_secs(60);
    actor.config.session.write_timeout = Duration::from_secs(60);
    actor.config.session.shutdown_timeout = Duration::from_secs(60);
    within(actor.apply(Input::OpenBytes(b"(;SZ[9];B[aa])".to_vec())))
        .await
        .unwrap();
    within(actor.apply(Input::ConfigureEngine(Some(profile))))
        .await
        .unwrap();
    let deep_a = final_analysis(&mut actor, &mut output).await;
    assert_eq!(deep_a.visits, 100);
    assert_eq!(deep_a.summary.visits, 100);
    let fingerprint = actor.summary_fingerprint.clone();

    within(actor.apply(Input::Edit(document::Command::Step(1))))
        .await
        .unwrap();
    let deep_b = final_analysis(&mut actor, &mut output).await;
    assert_eq!(deep_b.visits, 100);
    assert_ne!(deep_b.node_id, deep_a.node_id);

    // B's completed query has evicted A from the one-entry cache. Limit the
    // next query through the coordinator's real command API while paused.
    // Revisiting A must restore its document-owned payload before that query.
    // This is test-only access, with no production hook or profile change.
    within(actor.apply(Input::SetPondering(false)))
        .await
        .unwrap();
    let mut live = actor.config.analysis.live.clone();
    live.max_visits = 1;
    within(
        actor
            .running
            .as_ref()
            .unwrap()
            .coordinator
            .set_live_settings(live),
    )
    .await
    .unwrap();
    within(actor.apply(Input::Edit(document::Command::GoTo(NodeId(deep_a.node_id)))))
        .await
        .unwrap();
    within(actor.apply(Input::SetPondering(true)))
        .await
        .unwrap();
    let shallow_a = final_analysis(&mut actor, &mut output).await;
    assert_eq!(shallow_a.source, crate::AnalysisSource::Cache);
    assert_eq!(shallow_a.visits, deep_a.visits);
    assert_eq!(shallow_a.candidates, deep_a.candidates);
    assert!(!shallow_a.candidates.is_empty());
    assert_eq!(shallow_a.ownership.as_ref().unwrap().len(), 81);
    assert_eq!(shallow_a.summary, deep_a.summary);
    assert_eq!(actor.summary_fingerprint, fingerprint);

    actor.publish_document().unwrap();
    let Some(AppEvent::DocumentChanged { view, .. }) = event(&mut output).await else {
        panic!("graph was not published");
    };
    assert_eq!(view.graph.points[0].summary, Some(deep_a.summary));
    assert_eq!(view.graph.points[1].summary, Some(deep_b.summary));
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn every_analysis_is_forwarded_with_best_summary_and_fresh_values_on_ties() {
    let (mut actor, mut output) = actor();
    // Exercise live/cache, shallower results after coordinator replacement or
    // LRU eviction, and equal-depth replacements without timing a real search.
    for (visits, winrate, source, improves) in [
        (40, 0.7, analysis::SnapshotSource::Live, true),
        (12, 0.1, analysis::SnapshotSource::Live, false),
        (30, 0.2, analysis::SnapshotSource::Cache, false),
        (40, 0.8, analysis::SnapshotSource::Cache, true),
        (50, 0.9, analysis::SnapshotSource::Live, true),
    ] {
        let previous = actor.summaries.get(&NodeId(0)).copied();
        let mut incoming = snapshot(&actor, visits, winrate);
        incoming.source = source;
        actor.signal(Signal::Snapshot(Box::new(incoming)));
        // Every result, even a shallower one, must precede this document
        // barrier. Check live fields and graph history independently.
        actor.publish_document().unwrap();
        let Some(AppEvent::AnalysisUpdated(view)) = event(&mut output).await else {
            panic!("analysis was not forwarded");
        };
        assert_eq!((view.visits, view.winrate), (visits, winrate));
        assert_eq!(view.candidates[0].visits, visits);
        let expected = if improves {
            AnalysisSummary {
                visits: view.visits,
                winrate: view.winrate,
                score_lead: view.score_lead,
            }
        } else {
            previous.unwrap()
        };
        assert_eq!(view.summary, expected);
        let Some(AppEvent::DocumentChanged { view, .. }) = event(&mut output).await else {
            panic!("document barrier was not delivered");
        };
        assert_eq!(view.graph.points[0].summary, Some(expected));
    }
    drop(actor);
    assert_eq!(event(&mut output).await, None);
}

#[tokio::test]
async fn rejected_or_unpublished_snapshots_never_populate_the_graph() {
    for rejection in [
        "revision",
        "node",
        "key",
        "coordinate",
        "paused",
        "no focus",
    ] {
        let (mut actor, mut output) = actor();
        let mut incoming = snapshot(&actor, 100, 0.8);
        match rejection {
            "revision" => incoming.revision += 1,
            "node" => incoming.node_id = NodeId(1),
            "key" => incoming.key = AnalysisKey([0; 32]),
            "coordinate" => incoming.response.move_infos[0].pv.push("I4".into()),
            "paused" => actor.settings.pondering = false,
            "no focus" => actor.focus = None,
            _ => unreachable!(),
        }
        actor.signal(Signal::Snapshot(Box::new(incoming)));
        actor.publish_document().unwrap();
        drop(actor);
        let mut documents = 0;
        while let Some(event) = event(&mut output).await {
            match event {
                AppEvent::DocumentChanged { view, .. } => {
                    documents += 1;
                    assert_eq!(view.graph.points[0].summary, None, "{rejection}");
                }
                AppEvent::AnalysisUpdated(_) => panic!("forwarded {rejection} snapshot"),
                AppEvent::EngineStatusChanged(_)
                | AppEvent::WholeGameProgress(_)
                | AppEvent::BoardSyncStatusChanged(_) => {}
            }
        }
        assert_eq!(documents, 1);
    }
}
