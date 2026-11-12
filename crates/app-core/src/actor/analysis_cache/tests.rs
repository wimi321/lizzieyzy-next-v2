#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{
    AppEvent, EngineProfile,
    actor::{Input, next_signal},
};
use analysis::persistent_cache::{CacheStatus, DEFAULT_MAX_BYTES, PersistentCache};
use std::time::Duration;

async fn paused_actor(directory: &std::path::Path) -> (Actor, crate::AppSubscription) {
    let (mut actor, events) = crate::actor::tests::actor();
    actor.directory = directory.to_owned();
    actor.settings.pondering = false;
    actor.config.session.write_timeout = Duration::from_secs(60);
    actor.config.session.response_timeout = Duration::from_secs(60);
    actor.config.session.shutdown_timeout = Duration::from_secs(60);
    actor.cache = Some(PersistentCache::spawn(
        directory.join("cache.sqlite3"),
        DEFAULT_MAX_BYTES,
        true,
    ));
    actor
        .apply(Input::OpenBytes(b"(;SZ[9];B[aa])".to_vec()))
        .await
        .unwrap();
    let program = fake_katago::binary_path().unwrap();
    actor
        .apply(Input::ConfigureEngine(Some(EngineProfile {
            model: program.clone(),
            config: program.clone(),
            program,
            extra_args: Vec::new(),
            canonical_model_name: None,
        })))
        .await
        .unwrap();
    assert!(actor.running.is_some());
    (actor, events)
}

fn queue_latest(actor: &mut Actor, visits: u64) -> AnalysisResponse {
    // Keep the actual coordinator paused while supplying deterministic live
    // snapshots through the same acceptance/write path as the application.
    actor.settings.pondering = true;
    let earlier = crate::actor::tests::snapshot(actor, visits - 1, 0.5);
    actor.cache_signal(Signal::Snapshot(Box::new(earlier)));
    let mut latest = crate::actor::tests::snapshot(actor, visits, 0.7);
    latest.response.ownership = Some(vec![0.25; 81]);
    let response = latest.response.clone();
    actor.cache_signal(Signal::Snapshot(Box::new(latest)));
    actor.settings.pondering = false;
    assert_eq!(
        actor.review.results[&actor.document.current()].response,
        response
    );
    response
}

#[tokio::test(start_paused = true)]
async fn navigation_and_same_node_key_changes_flush_the_latest_pending_snapshot() {
    // A runnable task keeps Tokio from auto-advancing its paused clock while
    // SQLite/process I/O is pending. No cache timer can cause these writes.
    let clock_guard = tokio::spawn(async {
        loop {
            tokio::task::yield_now().await;
        }
    });
    let started = tokio::time::Instant::now();
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _events) = paused_actor(directory.path()).await;
    let reader = PersistentCache::spawn(
        directory.path().join("cache.sqlite3"),
        DEFAULT_MAX_BYTES,
        true,
    );
    let fingerprint = actor.cache_fingerprint().unwrap();
    let root = actor.focus.as_ref().unwrap().key;
    let latest = queue_latest(&mut actor, 100);
    assert_eq!(
        actor.cache.as_ref().unwrap().get(root, &fingerprint).await,
        Some(latest.clone())
    );
    assert_eq!(reader.get(root, &fingerprint).await, None);
    actor
        .apply(Input::Edit(document::Command::Step(1)))
        .await
        .unwrap();
    assert_eq!(reader.get(root, &fingerprint).await, Some(latest));

    let node = actor.document.current();
    let old_key = actor.focus.as_ref().unwrap().key;
    let latest = queue_latest(&mut actor, 200);
    assert_eq!(
        actor
            .cache
            .as_ref()
            .unwrap()
            .get(old_key, &fingerprint)
            .await,
        Some(latest.clone())
    );
    assert_eq!(reader.get(old_key, &fingerprint).await, None);
    let info = actor.document.game_info().unwrap();
    actor
        .apply(Input::Edit(document::Command::SetGameInfo(
            document::GameInfoEdit {
                black_name: info.black_name,
                black_rank: info.black_rank,
                white_name: info.white_name,
                white_rank: info.white_rank,
                komi: 6.5,
                rules: "chinese".into(),
                result: info.result,
                date: info.date,
                event: info.event,
            },
        )))
        .await
        .unwrap();
    assert_eq!(actor.document.current(), node);
    assert_ne!(actor.focus.as_ref().unwrap().key, old_key);
    assert_eq!(reader.get(old_key, &fingerprint).await, Some(latest));
    assert_eq!(tokio::time::Instant::now(), started);
    actor.stop_engine().await.unwrap();
    actor.cache.as_ref().unwrap().shutdown().await;
    reader.shutdown().await;
    clock_guard.abort();
}

#[tokio::test(start_paused = true)]
async fn replacing_the_same_document_flushes_the_latest_pending_snapshot() {
    let clock_guard = tokio::spawn(async {
        loop {
            tokio::task::yield_now().await;
        }
    });
    let started = tokio::time::Instant::now();
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _events) = paused_actor(directory.path()).await;
    let reader = PersistentCache::spawn(
        directory.path().join("cache.sqlite3"),
        DEFAULT_MAX_BYTES,
        true,
    );
    let fingerprint = actor.cache_fingerprint().unwrap();
    let node = actor.document.current();
    let key = actor.focus.as_ref().unwrap().key;
    let latest = queue_latest(&mut actor, 300);
    assert_eq!(
        actor.cache.as_ref().unwrap().get(key, &fingerprint).await,
        Some(latest.clone())
    );
    assert_eq!(reader.get(key, &fingerprint).await, None);

    actor
        .apply(Input::OpenBytes(b"(;SZ[9];B[aa])".to_vec()))
        .await
        .unwrap();
    assert_eq!(actor.document.current(), node);
    assert_eq!(actor.focus.as_ref().unwrap().key, key);
    assert_eq!(reader.get(key, &fingerprint).await, Some(latest));
    assert_eq!(tokio::time::Instant::now(), started);
    actor.stop_engine().await.unwrap();
    actor.cache.as_ref().unwrap().shutdown().await;
    reader.shutdown().await;
    clock_guard.abort();
}

#[tokio::test]
async fn invalid_cached_coordinates_disable_storage_without_failing_live_analysis() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, mut events) = crate::actor::tests::actor();
    actor.directory = directory.path().to_owned();
    actor.settings.pondering = false;
    actor.config.analysis.live.max_visits = 5;
    actor.config.analysis.live.report_interval = Duration::from_secs(60);
    actor.config.session.write_timeout = Duration::from_secs(60);
    actor.config.session.response_timeout = Duration::from_secs(60);
    actor.config.session.shutdown_timeout = Duration::from_secs(60);
    let cache = PersistentCache::spawn(
        directory.path().join("cache.sqlite3"),
        DEFAULT_MAX_BYTES,
        true,
    );
    actor.cache = Some(cache.clone());
    let program = fake_katago::binary_path().unwrap();
    actor
        .apply(Input::ConfigureEngine(Some(EngineProfile {
            model: program.clone(),
            config: program.clone(),
            program,
            extra_args: vec!["--visits-per-sec".into(), "100000".into()],
            canonical_model_name: None,
        })))
        .await
        .unwrap();
    let mut response = crate::actor::tests::snapshot(&actor, 100, 0.6).response;
    response.move_infos[0].coordinate = "I4".into();
    let position = actor.document.position_at(NodeId(0)).unwrap();
    cache.put(
        position.analysis_key(),
        &actor.cache_fingerprint().unwrap(),
        response,
    );
    assert!(actor.cached_focus(NodeId(0), &position).await.is_none());
    assert!(matches!(*cache.status().borrow(), CacheStatus::Failed(_)));
    actor.apply(Input::SetPondering(true)).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                signal = next_signal(&mut actor.running) => actor.cache_signal(signal),
                event = events.recv() => match event.unwrap() {
                    AppEvent::AnalysisUpdated(view) if view.source == crate::AnalysisSource::Live && !view.is_during_search => {
                        assert_eq!(view.visits, 5);
                        break;
                    }
                    AppEvent::EngineStatusChanged(crate::EngineStatus::Failed { reason, .. }) => panic!("engine failure: {reason}"),
                    _ => {}
                },
            }
        }
    }).await.unwrap();
    actor.stop_engine().await.unwrap();
    cache.shutdown().await;
}
