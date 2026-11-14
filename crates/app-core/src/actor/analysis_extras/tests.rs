#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::{
    Actor, Input, Signal, next_signal,
    tests::{actor, snapshot},
};
use crate::{
    AnalysisExtras, AnalysisPauseReason, AnalysisSource, AnalysisView, AppError, AppEvent,
    AppSubscription, BatchOptions, BatchOutput, BatchPhase, EngineProfile, WholeGameOptions,
    WholeGameScope, WholeGameStatus, batch, review::ReviewInput,
};
use document::{Command, NodeId};
use std::{future::Future, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("event-driven operation exceeded its safety deadline")
}

async fn prepared(pondering: bool) -> (Actor, AppSubscription, tempfile::TempDir) {
    // Build and prepare filesystem state before any safety deadline.
    let directory = tempfile::tempdir().unwrap();
    let program = fake_katago::binary_path().unwrap();
    let profile = EngineProfile {
        model: program.clone(),
        config: program.clone(),
        program,
        canonical_model_name: None,
        extra_args: vec!["--default-perspective-to-play".into()],
    };
    let (mut actor, output) = actor();
    actor.directory = directory.path().to_owned();
    actor.settings.pondering = pondering;
    actor.settings.auto_quick_analyze_on_load = false;
    actor.settings.cache_enabled = false;
    actor.config.analysis.live.max_visits = 20;
    actor.config.analysis.live.report_interval = Duration::from_secs(60);
    actor.config.session.startup_timeout = Duration::from_secs(60);
    actor.config.session.response_timeout = Duration::from_secs(60);
    actor.config.session.write_timeout = Duration::from_secs(60);
    actor.config.session.shutdown_timeout = Duration::from_secs(60);
    actor.config.session.max_restarts = 0;
    actor
        .apply(Input::OpenBytes(b"(;SZ[5:3];B[aa];W[ba])".to_vec()))
        .await
        .unwrap();
    actor.apply(Input::Edit(Command::Step(1))).await.unwrap();
    actor
        .apply(Input::ConfigureEngine(Some(profile)))
        .await
        .unwrap();
    (actor, output, directory)
}

async fn final_view(actor: &mut Actor, output: &mut AppSubscription) -> AnalysisView {
    within(async {
        loop {
            tokio::select! {
                signal = next_signal(&mut actor.running) => actor.signal(signal),
                event = output.recv() => match event.expect("event stream closed") {
                    AppEvent::AnalysisUpdated(view) => {
                        assert_eq!(view.node_id, actor.document.current().0, "stale node escaped");
                        assert_eq!(view.revision, actor.revision, "stale revision escaped");
                        if !view.is_during_search { return view; }
                    }
                    AppEvent::EngineStatusChanged(crate::EngineStatus::Failed { reason, .. }) => {
                        panic!("fake engine failed: {reason}");
                    }
                    _ => {}
                }
            }
        }
    })
    .await
}

fn extras() -> AnalysisExtras {
    AnalysisExtras {
        policy: true,
        moves_ownership: true,
    }
}

fn assert_white_extras(view: &AnalysisView) {
    let policy = view.policy.as_ref().expect("policy requested");
    assert_eq!(policy.len(), 16);
    assert_eq!(policy[0], -1.0);
    assert!(policy[15] > 0.0, "last policy value is pass");
    assert!(!view.candidates.is_empty());
    for candidate in &view.candidates {
        let ownership = candidate
            .ownership
            .as_ref()
            .expect("move ownership requested");
        assert_eq!(ownership.len(), 15);
        assert_eq!(
            ownership[0], 1.0,
            "Black's A3 stone stays positive with White to play"
        );
        let point = candidate.point.as_ref().expect("empty points available");
        assert_eq!(
            ownership[usize::from(point.y) * 5 + usize::from(point.x)],
            -1.0
        );
    }
}

#[tokio::test]
async fn paused_extras_fill_one_visit_and_keep_black_perspective_and_pause() {
    let (mut actor, mut output, _directory) = prepared(false).await;
    let before = actor.settings.clone();
    within(actor.apply(Input::SetAnalysisExtras(extras())))
        .await
        .unwrap();
    let view = final_view(&mut actor, &mut output).await;
    assert_eq!(view.visits, 1);
    assert_white_extras(&view);
    assert!(!actor.analysis_state().pondering);
    assert_eq!(
        actor.analysis_state().pause_reason,
        Some(AnalysisPauseReason::User)
    );
    assert!(!actor.analysis_state().pure_network);
    assert_eq!(actor.settings, before, "extras are transient runtime state");
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn supplemental_extras_preserve_deeper_ordinary_values_and_pause() {
    let (mut actor, mut output, _directory) = prepared(true).await;
    let ordinary = final_view(&mut actor, &mut output).await;
    assert_eq!(ordinary.visits, 20);
    assert!(ordinary.policy.is_none());
    assert!(
        ordinary
            .candidates
            .iter()
            .all(|candidate| candidate.ownership.is_none())
    );
    actor.apply(Input::SetPondering(false)).await.unwrap();
    within(actor.apply(Input::SetAnalysisExtras(extras())))
        .await
        .unwrap();
    let supplemented = final_view(&mut actor, &mut output).await;
    assert_white_extras(&supplemented);
    assert_eq!(supplemented.visits, ordinary.visits);
    assert_eq!(supplemented.winrate, ordinary.winrate);
    assert_eq!(supplemented.score_lead, ordinary.score_lead);
    assert_eq!(supplemented.summary, ordinary.summary);
    assert_eq!(
        actor.analysis_state().pause_reason,
        Some(AnalysisPauseReason::User)
    );
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn pure_network_navigates_once_and_restores_running_or_paused_intent() {
    for pondering in [false, true] {
        let (mut actor, mut output, _directory) = prepared(pondering).await;
        if pondering {
            let ordinary = final_view(&mut actor, &mut output).await;
            assert_eq!(ordinary.visits, 20);
        }
        let before_results = actor.review.results.clone();
        let before_summaries = actor.summaries.clone();
        let before_settings = actor.settings.clone();
        within(actor.apply(Input::SetPureNetwork(true)))
            .await
            .unwrap();
        let pure = final_view(&mut actor, &mut output).await;
        assert_eq!(pure.visits, 1);
        assert!(pure.policy.is_some());
        assert_eq!(pure.ownership.as_ref().unwrap().len(), 15);
        assert_eq!(pure.ownership.as_ref().unwrap()[0], 1.0);
        assert!(actor.analysis_state().pure_network);
        assert!(!actor.analysis_state().pondering);
        assert_eq!(
            actor.review.results, before_results,
            "pure output must not replace retained analysis"
        );
        assert_eq!(
            actor.summaries, before_summaries,
            "pure output must not change graph history"
        );

        within(actor.apply(Input::Edit(Command::Step(1))))
            .await
            .unwrap();
        let next = final_view(&mut actor, &mut output).await;
        assert_ne!(next.node_id, pure.node_id);
        assert_eq!(next.visits, 1);
        assert_eq!(&next.policy.as_ref().unwrap()[..2], &[-1.0, -1.0]);
        assert_eq!(actor.review.results, before_results);
        assert_eq!(actor.summaries, before_summaries);
        within(actor.apply(Input::SetPureNetwork(false)))
            .await
            .unwrap();
        assert!(!actor.analysis_state().pure_network);
        assert_eq!(actor.analysis_state().pondering, pondering);
        assert_eq!(
            actor.settings, before_settings,
            "pure mode is never persisted"
        );
        if pondering {
            let resumed = final_view(&mut actor, &mut output).await;
            assert_eq!(resumed.visits, 20);
            assert_eq!(resumed.source, AnalysisSource::Live);
            assert!(resumed.policy.is_none());
        } else {
            assert_eq!(
                actor.analysis_state().pause_reason,
                Some(AnalysisPauseReason::User)
            );
            assert_eq!(actor.review.results, before_results);
        }
        within(actor.stop_engine()).await.unwrap();
    }
}

fn batch_options() -> BatchOptions {
    BatchOptions {
        files: vec!["queued.sgf".into()],
        visits: 1,
        scope: WholeGameScope::Line,
        force: true,
        output: BatchOutput {
            suffix: "analyzed".into(),
            directory: None,
        },
    }
}

#[tokio::test]
async fn pure_network_and_whole_game_or_batch_are_mutually_exclusive() {
    let (mut actor, _output, _directory) = prepared(false).await;
    let options = WholeGameOptions {
        visits: 1,
        force: true,
        ..Default::default()
    };
    actor
        .review_command(ReviewInput::Start(options))
        .await
        .unwrap();
    // No review signal is consumed: Running and Paused are command barriers,
    // independent of how quickly the fake process computes a final result.
    for expected in [WholeGameStatus::Running, WholeGameStatus::Paused] {
        if expected == WholeGameStatus::Paused {
            actor.review_command(ReviewInput::Pause).await.unwrap();
        }
        assert_eq!(actor.review.progress.as_ref().unwrap().status, expected);
        assert!(matches!(
            actor.set_pure_network(true).await,
            Err(AppError::WholeGameAnalysisActive)
        ));
        assert!(!actor.config.analysis.pure_network);
    }
    actor.review_command(ReviewInput::Cancel).await.unwrap();
    actor
        .batch_command(batch::Request::Start(batch_options()))
        .await
        .unwrap();
    for expected in [BatchPhase::Running, BatchPhase::Paused] {
        if expected == BatchPhase::Paused {
            actor.batch_command(batch::Request::Pause).await.unwrap();
        }
        assert_eq!(actor.batch.status.phase, expected);
        assert!(matches!(
            actor.set_pure_network(true).await,
            Err(AppError::BatchAnalysisRunning)
        ));
        assert!(!actor.config.analysis.pure_network);
    }
    actor.batch_command(batch::Request::Stop).await.unwrap();
    actor.set_pure_network(true).await.unwrap();
    assert!(matches!(
        actor.review_command(ReviewInput::Start(options)).await,
        Err(AppError::PureNetworkActive)
    ));
    assert!(matches!(
        actor
            .batch_command(batch::Request::Start(batch_options()))
            .await,
        Err(AppError::PureNetworkActive)
    ));
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn replaced_extras_and_pure_results_never_escape_to_the_new_focus() {
    let (mut actor, mut output, _directory) = prepared(false).await;
    actor.set_analysis_extras(extras()).await.unwrap();
    let mut stale_extras = snapshot(&actor, 1000, 0.99);
    stale_extras.source = analysis::SnapshotSource::Extras;
    actor.apply(Input::Edit(Command::Step(1))).await.unwrap();
    actor.signal(Signal::Snapshot(Box::new(stale_extras)));
    let extras_view = final_view(&mut actor, &mut output).await;
    assert_eq!(extras_view.visits, 1);
    assert!(!actor.review.results.contains_key(&NodeId(1)));
    actor.set_pure_network(true).await.unwrap();
    let mut stale_pure = snapshot(&actor, 1000, 0.99);
    stale_pure.source = analysis::SnapshotSource::PureNetwork;
    actor.apply(Input::Edit(Command::Step(-1))).await.unwrap();
    actor.signal(Signal::Snapshot(Box::new(stale_pure)));
    let current = final_view(&mut actor, &mut output).await;
    assert_eq!(current.visits, 1);
    assert_eq!(current.node_id, 1);
    actor.set_pure_network(false).await.unwrap();
    let retained = actor.review.results.clone();
    let mut wrong_mode = snapshot(&actor, 1000, 0.99);
    wrong_mode.source = analysis::SnapshotSource::PureNetwork;
    actor.signal(Signal::Snapshot(Box::new(wrong_mode)));
    assert_eq!(actor.review.results, retained);
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn saved_overlays_survive_republication_and_pure_never_replaces_saved_history() {
    const JAVA: &[u8] = include_bytes!("../../../../../fixtures/lz/java-analysis.sgf");
    let (mut actor, mut output) = actor();
    actor.settings.pondering = false;
    actor.settings.auto_quick_analyze_on_load = false;
    actor.open(JAVA.to_vec(), None).await.unwrap();
    let position = actor.document.position_at(NodeId(0)).unwrap();
    actor.focus = Some(super::super::ViewFocus {
        revision: actor.revision,
        node_id: 0,
        key: position.analysis_key(),
        size: position.board.size(),
        to_play: position.board.to_play(),
    });
    let original = actor.stored_analysis(0).unwrap();
    let original_export = actor.export_document_with_analysis().unwrap().0;
    let original_summaries = actor.summaries.clone();
    let mut supplemental = snapshot(&actor, 1, 0.99);
    supplemental.source = analysis::SnapshotSource::Extras;
    supplemental.response = original.response.clone();
    supplemental.response.root_info.visits = 1;
    supplemental.response.root_info.winrate = 0.99;
    supplemental.response.policy = Some(vec![1.0 / 26.0; 26]);
    for candidate in &mut supplemental.response.move_infos {
        candidate.ownership = Some(vec![0.75; 25]);
    }
    actor.signal(Signal::Snapshot(Box::new(supplemental.clone())));
    let overlay = final_view(&mut actor, &mut output).await;
    assert_eq!(overlay.source, AnalysisSource::Saved);
    assert_eq!(overlay.visits, original.response.root_info.visits);
    assert_eq!(overlay.winrate, original.response.root_info.winrate);
    assert_eq!(overlay.policy, supplemental.response.policy);
    assert!(
        overlay
            .candidates
            .iter()
            .all(|candidate| candidate.ownership == Some(vec![0.75; 25]))
    );
    assert_eq!(actor.stored_analysis(0).unwrap().saved, original.saved);
    assert_eq!(actor.summaries, original_summaries);
    assert_eq!(
        actor.export_document_with_analysis().unwrap().0,
        original_export
    );

    actor.publish_document().unwrap();
    let republished = final_view(&mut actor, &mut output).await;
    assert_eq!(
        republished, overlay,
        "document refresh retains requested overlays"
    );

    actor.set_pure_network(true).await.unwrap();
    let before_pure = actor.review.results.clone();
    supplemental.source = analysis::SnapshotSource::PureNetwork;
    actor.signal(Signal::Snapshot(Box::new(supplemental)));
    let pure = final_view(&mut actor, &mut output).await;
    assert_eq!(pure.visits, 1);
    assert_eq!(pure.winrate, 0.99);
    assert_eq!(pure.source, AnalysisSource::Live);
    assert_eq!(actor.review.results, before_pure);
    assert_eq!(actor.summaries, original_summaries);
    assert_eq!(
        actor.export_document_with_analysis().unwrap().0,
        original_export
    );
    actor.set_pure_network(false).await.unwrap();
    let restored = final_view(&mut actor, &mut output).await;
    assert_eq!(restored, overlay);
}
