#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{collections::BTreeMap, ffi::OsString, future::Future, time::Duration};

use analysis::{
    AnalysisConfig, AnalysisHandle, AnalysisLimits, AnalysisStatus, Focus, LimitReason,
    LiveSettings, SnapshotSource,
};
use document::{Command, GameDocument};
use engine_session::{EngineHandle, SessionConfig, TransportSpec};
use go_core::Move;
use tokio::time::{advance, pause, resume, timeout};

async fn within<T>(future: impl Future<Output = T>) -> T {
    timeout(Duration::from_secs(5), future)
        .await
        .expect("event exceeded safety deadline")
}

fn engine() -> EngineHandle {
    engine_session::spawn(
        TransportSpec::LocalProcess {
            program: fake_katago::binary_path().unwrap(),
            args: [
                "analysis",
                "-config",
                "unused.cfg",
                "-model",
                "unused.bin.gz",
                "--visits-per-sec",
                "1000",
                "--threads",
                "2",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            cwd: None,
            env: BTreeMap::new(),
        },
        SessionConfig {
            startup_timeout: Duration::from_secs(86_400),
            response_timeout: Duration::from_secs(86_400),
            ..SessionConfig::default()
        },
    )
}

fn focus(document: &GameDocument) -> Focus {
    let position = document.position_at(document.current()).unwrap();
    Focus {
        revision: document.revision(),
        node_id: document.current(),
        key: position.analysis_key(),
        position,
    }
}

async fn status(handle: &AnalysisHandle, expected: AnalysisStatus) {
    let mut status = handle.status();
    within(async {
        loop {
            let current = status.borrow_and_update().clone();
            if current == expected {
                return;
            }
            assert!(
                !matches!(current, AnalysisStatus::Failed(_) | AnalysisStatus::Stopped),
                "unexpected status: {current:?}"
            );
            status.changed().await.unwrap();
        }
    })
    .await;
}

#[tokio::test]
async fn time_limit_stops_fake_search_with_paused_time_and_explicit_resume_restarts_it() {
    let engine = engine();
    let (handle, mut output) = analysis::spawn(
        engine.clone(),
        AnalysisConfig {
            limits: AnalysisLimits {
                max_analyze_time_seconds: 2,
                ..AnalysisLimits::default()
            },
            live: LiveSettings {
                report_interval: Duration::from_millis(10),
                ..LiveSettings::default()
            },
            ..AnalysisConfig::default()
        },
    )
    .unwrap();
    let focused = focus(&GameDocument::new());
    within(handle.focus(focused.clone())).await.unwrap();
    let first = within(output.recv()).await.unwrap();
    assert_eq!(first.source, SnapshotSource::Live);
    pause();
    advance(Duration::from_secs(2)).await;
    resume();
    status(&handle, AnalysisStatus::Limited(LimitReason::Time)).await;
    within(handle.set_pondering(true)).await.unwrap();
    within(handle.focus(focused)).await.unwrap();
    assert_eq!(
        *handle.status().borrow(),
        AnalysisStatus::Limited(LimitReason::Time)
    );
    within(handle.resume()).await.unwrap();
    status(&handle, AnalysisStatus::Analyzing).await;
    let after = within(async {
        loop {
            let value = output.recv().await.unwrap();
            if value.response.id != first.response.id {
                return value;
            }
        }
    })
    .await;
    assert_eq!(after.node_id, first.node_id);
    within(handle.shutdown()).await.unwrap();
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn playout_limit_keeps_triggering_result_and_resume_exempts_only_current_position() {
    let engine = engine();
    let (handle, mut output) = analysis::spawn(
        engine.clone(),
        AnalysisConfig {
            limits: AnalysisLimits {
                limit_time: false,
                limit_playout: true,
                limit_playouts: 10,
                ..AnalysisLimits::default()
            },
            live: LiveSettings {
                max_visits: 100,
                report_interval: Duration::from_millis(10),
                ..LiveSettings::default()
            },
            ..AnalysisConfig::default()
        },
    )
    .unwrap();
    let mut document = GameDocument::new();
    within(handle.focus(focus(&document))).await.unwrap();
    status(&handle, AnalysisStatus::Limited(LimitReason::Playouts)).await;
    let last = within(async {
        loop {
            let value = output.recv().await.unwrap();
            if value.response.root_info.visits > 10 {
                return value;
            }
        }
    })
    .await;
    assert_eq!(last.source, SnapshotSource::Live);
    within(handle.resume()).await.unwrap();
    status(&handle, AnalysisStatus::Idle).await;
    let final_result = within(async {
        loop {
            let value = output.recv().await.unwrap();
            if !value.response.is_during_search {
                return value;
            }
        }
    })
    .await;
    assert_eq!(final_result.response.root_info.visits, 100);

    document
        .apply(Command::Play(Move::Pass(go_core::Color::Black)))
        .unwrap();
    within(handle.focus(focus(&document))).await.unwrap();
    status(&handle, AnalysisStatus::Limited(LimitReason::Playouts)).await;
    within(handle.shutdown()).await.unwrap();
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn foreground_limit_does_not_terminate_whole_game_queries() {
    let engine = engine();
    let (handle, _) = analysis::spawn(
        engine.clone(),
        AnalysisConfig {
            limits: AnalysisLimits {
                limit_time: false,
                limit_playout: true,
                limit_playouts: 1,
                ..AnalysisLimits::default()
            },
            live: LiveSettings {
                report_interval: Duration::from_millis(10),
                ..LiveSettings::default()
            },
            ..AnalysisConfig::default()
        },
    )
    .unwrap();
    let mut document = GameDocument::new();
    let mut nodes = vec![(
        document.current(),
        document.position_at(document.current()).unwrap(),
    )];
    for color in [go_core::Color::Black, go_core::Color::White] {
        document.apply(Command::Play(Move::Pass(color))).unwrap();
        nodes.push((
            document.current(),
            document.position_at(document.current()).unwrap(),
        ));
    }
    within(handle.focus(focus(&document))).await.unwrap();
    let plan = analysis::review::ReviewPlan::new(nodes, 500).unwrap();
    let mut count = 0;
    for batch in plan.into_batches() {
        let mut run = within(batch.submit(&engine)).await.unwrap();
        while let Some(result) = within(run.recv()).await {
            let results = result.unwrap();
            for result in &results {
                assert_eq!(result.response.root_info.visits, 500);
            }
            count += results.len();
        }
    }
    assert_eq!(count, 3);
    status(&handle, AnalysisStatus::Limited(LimitReason::Playouts)).await;
    within(handle.shutdown()).await.unwrap();
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn atomic_focus_resume_does_not_clear_a_new_positions_playout_stop() {
    let engine = engine();
    let (handle, _) = analysis::spawn(
        engine.clone(),
        AnalysisConfig {
            limits: AnalysisLimits {
                limit_time: false,
                limit_playout: true,
                limit_playouts: 1,
                ..AnalysisLimits::default()
            },
            live: LiveSettings {
                report_interval: Duration::from_millis(1),
                ..LiveSettings::default()
            },
            ..AnalysisConfig::default()
        },
    )
    .unwrap();
    let mut document = GameDocument::new();
    within(handle.focus(focus(&document))).await.unwrap();
    status(&handle, AnalysisStatus::Limited(LimitReason::Playouts)).await;
    document
        .apply(Command::Play(Move::Pass(go_core::Color::Black)))
        .unwrap();
    within(handle.focus_with_cached_and_resume(focus(&document), None, true))
        .await
        .unwrap();
    status(&handle, AnalysisStatus::Limited(LimitReason::Playouts)).await;
    within(handle.set_pondering(true)).await.unwrap();
    assert_eq!(
        *handle.status().borrow(),
        AnalysisStatus::Limited(LimitReason::Playouts)
    );
    within(handle.shutdown()).await.unwrap();
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn saturated_command_stream_cannot_starve_the_limit_timer() {
    let engine = engine();
    let (handle, mut output) = analysis::spawn(
        engine.clone(),
        AnalysisConfig {
            limits: AnalysisLimits {
                max_analyze_time_seconds: 1,
                ..AnalysisLimits::default()
            },
            live: LiveSettings {
                report_interval: Duration::from_millis(10),
                ..LiveSettings::default()
            },
            ..AnalysisConfig::default()
        },
    )
    .unwrap();
    within(handle.focus(focus(&GameDocument::new())))
        .await
        .unwrap();
    within(output.recv()).await.unwrap();
    // More producers than the bounded command channel can hold: commands stay
    // ready while the actor executes them. Each producer first reports that it
    // has received an acknowledgement, so the test needs no timing sleeps.
    let (ack, mut acknowledgements) = tokio::sync::mpsc::channel(128);
    let mut producers = Vec::new();
    for _ in 0..128 {
        let handle = handle.clone();
        let ack = ack.clone();
        producers.push(tokio::spawn(async move {
            handle.set_pondering(true).await.unwrap();
            ack.send(()).await.unwrap();
            while handle.set_pondering(true).await.is_ok() {}
        }));
    }
    within(async {
        for _ in 0..128 {
            acknowledgements.recv().await.unwrap();
        }
    })
    .await;
    pause();
    advance(Duration::from_secs(1)).await;
    resume();
    status(&handle, AnalysisStatus::Limited(LimitReason::Time)).await;
    for producer in producers {
        producer.abort();
        let _ = producer.await;
    }
    within(handle.shutdown()).await.unwrap();
    within(engine.shutdown()).await.unwrap();
}
