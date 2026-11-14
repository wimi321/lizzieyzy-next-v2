#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{collections::BTreeMap, ffi::OsString};

use document::{Command, GameDocument};
use engine_session::{EngineHandle, SessionConfig, TransportSpec};
use go_core::Move;
use tempfile::TempDir;

use super::*;
use crate::{AnalysisConfig, AnalysisSnapshot, Focus, LiveSettings, SnapshotSource};

pub(super) mod safety_deadline;
use safety_deadline::within;

fn engine() -> EngineHandle {
    engine_session::spawn(
        TransportSpec::LocalProcess {
            program: fake_katago::binary_path().expect("build fake KataGo"),
            args: [
                "analysis",
                "-config",
                "unused.cfg",
                "-model",
                "unused.bin.gz",
                "--visits-per-sec",
                "100000",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            cwd: None,
            env: BTreeMap::new(),
        },
        SessionConfig::default(),
    )
}

fn focus(plies: u8) -> Focus {
    let mut doc = GameDocument::new();
    for _ in 0..plies {
        let color = doc.position_at(doc.current()).unwrap().board.to_play();
        doc.apply(Command::Play(Move::Pass(color))).unwrap();
    }
    let position = doc.position_at(doc.current()).unwrap();
    Focus {
        revision: u64::from(plies),
        node_id: doc.current(),
        key: position.analysis_key(),
        position,
    }
}

async fn analysis(engine: &EngineHandle, focus: Focus, visits: u64) -> AnalysisSnapshot {
    let (handle, mut output) = crate::spawn(
        engine.clone(),
        AnalysisConfig {
            live: LiveSettings {
                max_visits: visits,
                ..LiveSettings::default()
            },
            ..AnalysisConfig::default()
        },
    )
    .unwrap();
    within(handle.focus(focus)).await.unwrap();
    let result = within(async {
        loop {
            let snapshot = output.recv().await.unwrap();
            if !snapshot.response.is_during_search {
                break snapshot;
            }
        }
    })
    .await;
    within(handle.shutdown()).await.unwrap();
    result
}

fn cache(directory: &TempDir, enabled: bool) -> PersistentCache {
    PersistentCache::spawn(
        directory.path().join("cache.sqlite3"),
        DEFAULT_MAX_BYTES,
        enabled,
    )
}

#[tokio::test]
async fn persisted_fake_result_survives_reopen_and_never_loses_depth() {
    let directory = TempDir::new().unwrap();
    let engine = engine();
    let shallow = analysis(&engine, focus(0), 100).await;
    let deep = analysis(&engine, focus(0), 200).await;
    let store = cache(&directory, true);
    assert!(store.put(shallow.key, "model-a", shallow.response.clone()));
    assert!(store.put(deep.key, "model-a", deep.response.clone()));
    assert!(store.put(shallow.key, "model-a", shallow.response));
    within(store.flush()).await;
    let mut equal = deep.response.clone();
    equal.id = "latest-equal-depth".into();
    assert!(store.put(deep.key, "model-a", equal.clone()));
    assert_eq!(
        within(store.get(deep.key, "model-a")).await,
        Some(equal.clone())
    );
    assert_eq!(within(store.get(deep.key, "model-b")).await, None);
    within(store.shutdown()).await;

    let reopened = cache(&directory, true);
    assert_eq!(within(reopened.get(deep.key, "model-a")).await, Some(equal));
    within(reopened.shutdown()).await;
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn disabled_cache_neither_opens_nor_reads_nor_writes_and_reenables_existing_data() {
    let directory = TempDir::new().unwrap();
    let engine = engine();
    let shallow = analysis(&engine, focus(0), 100).await;
    let deep = analysis(&engine, focus(0), 200).await;
    let store = cache(&directory, false);
    assert!(store.put(shallow.key, "model", shallow.response.clone()));
    assert_eq!(within(store.get(shallow.key, "model")).await, None);
    assert!(!directory.path().join("cache.sqlite3").exists());
    within(store.set_enabled(true)).await;
    assert!(store.put(shallow.key, "model", shallow.response.clone()));
    within(store.set_enabled(false)).await;
    assert!(store.put(deep.key, "model", deep.response));
    assert_eq!(within(store.get(shallow.key, "model")).await, None);
    within(store.set_enabled(true)).await;
    assert_eq!(
        within(store.get(shallow.key, "model")).await,
        Some(shallow.response)
    );
    within(store.shutdown()).await;
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn lru_eviction_respects_database_file_size_and_reads_refresh_recency() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("cache.sqlite3");
    let engine = engine();
    let snapshot = analysis(&engine, focus(0), 100).await;
    let limit = 64 * 1024;
    let store = PersistentCache::spawn(path.clone(), limit, true);

    // Two 20 KiB payloads fit, but a third forces eviction. Padding only the
    // request ID preserves the fake engine's analysis while avoiding dozens of
    // disk writes merely to fill the database on slow Windows CI machines.
    let mut response = snapshot.response;
    response.id.clear();
    let padding = 20 * 1024 - serde_json::to_vec(&response).unwrap().len();
    response.id = "x".repeat(padding);
    assert_eq!(serde_json::to_vec(&response).unwrap().len(), 20 * 1024);
    let oldest = snapshot.key;
    let untouched = focus(1).key;
    let newest = focus(2).key;
    for key in [oldest, untouched] {
        assert!(store.put(key, "model", response.clone()));
        within(store.flush()).await;
        assert!(std::fs::metadata(&path).unwrap().len() <= limit);
        assert_eq!(*store.status().borrow(), CacheStatus::Enabled);
    }
    assert!(within(store.get(untouched, "model")).await.is_some());
    // Refresh the oldest insertion last: insertion order alone must not decide
    // which entry is evicted when the third payload exceeds the physical cap.
    assert!(within(store.get(oldest, "model")).await.is_some());
    assert!(store.put(newest, "model", response));
    within(store.flush()).await;
    let occupied_bytes = std::fs::metadata(&path).unwrap().len();
    assert!(occupied_bytes <= limit);
    assert_eq!(*store.status().borrow(), CacheStatus::Enabled);
    assert!(within(store.get(untouched, "model")).await.is_none());
    assert!(within(store.get(oldest, "model")).await.is_some());
    assert!(within(store.get(newest, "model")).await.is_some());
    within(store.clear_all()).await;
    assert!(std::fs::metadata(&path).unwrap().len() < occupied_bytes);
    within(store.shutdown()).await;
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn clear_position_covers_all_engines_and_global_clear_covers_other_positions() {
    let directory = TempDir::new().unwrap();
    let engine = engine();
    let snapshot = analysis(&engine, focus(0), 100).await;
    let other = analysis(&engine, focus(1), 100).await;
    let store = cache(&directory, true);
    for fingerprint in ["model-a", "model-b"] {
        assert!(store.put(snapshot.key, fingerprint, snapshot.response.clone()));
    }
    assert!(store.put(other.key, "model-a", other.response.clone()));
    within(store.set_enabled(false)).await;
    within(store.clear_position(snapshot.key)).await;
    within(store.set_enabled(true)).await;
    for fingerprint in ["model-a", "model-b"] {
        assert_eq!(within(store.get(snapshot.key, fingerprint)).await, None);
    }
    assert_eq!(
        within(store.get(other.key, "model-a")).await,
        Some(other.response)
    );
    within(store.clear_all()).await;
    assert_eq!(within(store.get(other.key, "model-a")).await, None);
    within(store.shutdown()).await;
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn corrupt_and_unwritable_databases_disable_cache_without_stopping_fake_analysis() {
    let directory = TempDir::new().unwrap();
    let corrupt = directory.path().join("corrupt.sqlite3");
    std::fs::write(&corrupt, b"not a SQLite database").unwrap();
    let impossible = directory.path().join("missing-parent/cache.sqlite3");
    std::fs::write(impossible.parent().unwrap(), b"parent is a file").unwrap();
    let engine = engine();
    for path in [corrupt, impossible] {
        let store = PersistentCache::spawn(path, DEFAULT_MAX_BYTES, true);
        assert!(within(store.get(focus(0).key, "model")).await.is_none());
        let failure = store.status().borrow().clone();
        assert!(matches!(failure, CacheStatus::Failed(_)));
        let snapshot = analysis(&engine, focus(0), 100).await;
        assert_eq!(snapshot.source, SnapshotSource::Live);
        assert_eq!(snapshot.response.root_info.visits, 100);
        assert!(store.put(snapshot.key, "model", snapshot.response));
        within(store.set_enabled(false)).await;
        within(store.set_enabled(true)).await;
        assert_eq!(*store.status().borrow(), failure);
        within(store.shutdown()).await;
    }
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn disabled_coordinator_does_not_reuse_seeds_or_capture_live_results() {
    let engine = engine();
    let (handle, mut output) = crate::spawn(
        engine.clone(),
        AnalysisConfig {
            cache_enabled: false,
            live: LiveSettings {
                max_visits: 100,
                ..LiveSettings::default()
            },
            ..AnalysisConfig::default()
        },
    )
    .unwrap();
    let original = focus(0);
    within(handle.focus(original.clone())).await.unwrap();
    let live = within(output.recv()).await.unwrap();
    assert_eq!(live.source, SnapshotSource::Live);
    within(handle.set_pondering(false)).await.unwrap();
    let mut revisit = original.clone();
    revisit.node_id = document::NodeId(100);
    revisit.revision = 1;
    within(handle.focus_with_cached(revisit.clone(), Some(live.response.clone())))
        .await
        .unwrap();
    tokio::select! {
        biased;
        result = output.recv() => panic!("disabled cache emitted {result:?}"),
        () = std::future::ready(()) => {}
    }
    within(handle.set_cache_enabled(true)).await.unwrap();
    revisit.node_id = original.node_id;
    revisit.revision = 2;
    within(handle.focus(revisit)).await.unwrap();
    tokio::select! {
        biased;
        result = output.recv() => panic!("disabled cache captured live result {result:?}"),
        () = std::future::ready(()) => {}
    }
    within(handle.shutdown()).await.unwrap();
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn database_write_failure_after_open_isolated_from_live_analysis() {
    let directory = TempDir::new().unwrap();
    let engine = engine();
    let store = cache(&directory, true);
    let first = analysis(&engine, focus(0), 100).await;
    assert!(store.put(first.key, "model", first.response.clone()));
    within(store.flush()).await;
    let path = directory.path().join("cache.sqlite3");
    let blocker = within(tokio::task::spawn_blocking(move || {
        let connection = rusqlite::Connection::open(path).unwrap();
        connection.execute_batch("BEGIN IMMEDIATE").unwrap();
        connection
    }))
    .await
    .unwrap();
    let later = analysis(&engine, focus(0), 200).await;
    assert!(store.put(later.key, "model", later.response));
    within(store.flush()).await;
    assert!(matches!(*store.status().borrow(), CacheStatus::Failed(_)));
    let still_live = analysis(&engine, focus(1), 100).await;
    assert_eq!(still_live.source, SnapshotSource::Live);
    assert_eq!(still_live.response.root_info.visits, 100);
    within(tokio::task::spawn_blocking(move || {
        blocker.execute_batch("ROLLBACK").unwrap();
    }))
    .await
    .unwrap();
    within(store.shutdown()).await;
    let reopened = cache(&directory, true);
    assert_eq!(
        within(reopened.get(first.key, "model")).await,
        Some(first.response)
    );
    within(reopened.shutdown()).await;
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn caller_detected_invalid_payload_disables_cache_until_new_actor() {
    let directory = TempDir::new().unwrap();
    let engine = engine();
    let snapshot = analysis(&engine, focus(0), 100).await;
    let store = cache(&directory, true);
    assert!(store.put(snapshot.key, "model", snapshot.response.clone()));
    within(store.flush()).await;
    within(store.invalidate("invalid candidate coordinate")).await;
    let failure = store.status().borrow().clone();
    assert_eq!(
        failure,
        CacheStatus::Failed("invalid candidate coordinate".into())
    );
    within(store.invalidate("second failure must not replace first")).await;
    within(store.set_enabled(true)).await;
    assert_eq!(*store.status().borrow(), failure);
    assert_eq!(within(store.get(snapshot.key, "model")).await, None);
    within(store.shutdown()).await;
    let reopened = cache(&directory, true);
    assert_eq!(
        within(reopened.get(snapshot.key, "model")).await,
        Some(snapshot.response)
    );
    within(reopened.shutdown()).await;
    within(engine.shutdown()).await.unwrap();
}
