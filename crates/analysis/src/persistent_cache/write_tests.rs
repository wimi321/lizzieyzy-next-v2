#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{future::Future, path::Path, time::Duration};

use tempfile::TempDir;

use super::*;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("cache operation exceeded safety deadline")
}

fn response(visits: u64) -> AnalysisResponse {
    serde_json::from_value(serde_json::json!({
        "id": format!("snapshot-{visits}"), "turnNumber": 0,
        "isDuringSearch": true, "moveInfos": [],
        "ownership": [0.1, 0.2, -0.3],
        "rootInfo": {"visits": visits, "winrate": 0.7, "scoreLead": 3.5,
                     "utility": 0.4, "currentPlayer": "B"}
    }))
    .unwrap()
}

fn cache(directory: &TempDir) -> PersistentCache {
    PersistentCache::spawn(
        directory.path().join("cache.sqlite3"),
        DEFAULT_MAX_BYTES,
        true,
    )
}

async fn pending_count(store: &PersistentCache) -> usize {
    let (reply, received) = oneshot::channel();
    store.commands.send(Command::Barrier(reply)).await.unwrap();
    within(received).await.unwrap()
}

async fn audit_writes(path: &Path, install: bool) -> usize {
    let path = path.to_owned();
    within(tokio::task::spawn_blocking(move || {
        let connection = rusqlite::Connection::open(path).unwrap();
        if install {
            connection
                .execute_batch(
                    "CREATE TABLE write_audit (n INTEGER NOT NULL);
                 INSERT INTO write_audit VALUES (0);
                 CREATE TRIGGER count_insert AFTER INSERT ON entries BEGIN
                     UPDATE write_audit SET n=n+1;
                 END;
                 CREATE TRIGGER count_payload_update AFTER UPDATE OF payload ON entries BEGIN
                     UPDATE write_audit SET n=n+1;
                 END;",
                )
                .unwrap();
        }
        connection
            .query_row("SELECT n FROM write_audit", [], |row| row.get(0))
            .unwrap()
    }))
    .await
    .unwrap()
}

#[tokio::test(start_paused = true)]
async fn hundred_received_snapshots_coalesce_and_timer_writes_once_per_window() {
    // Keep time under explicit control even while SQLite runs on its worker.
    let clock_guard = tokio::spawn(async {
        loop {
            tokio::task::yield_now().await;
        }
    });
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("cache.sqlite3");
    let store = cache(&directory);
    let key = AnalysisKey([1; 32]);
    assert_eq!(within(store.get(key, "model")).await, None);
    assert_eq!(audit_writes(&path, true).await, 0);
    for visits in 1..=100 {
        assert!(store.put(key, "model", response(visits)));
        // Actor receives every snapshot; the bound is not caused by dropping
        // a producer-only burst or by filling the incoming channel.
        assert_eq!(pending_count(&store).await, 1);
    }
    assert_eq!(within(store.get(key, "model")).await, Some(response(100)));
    assert_eq!(audit_writes(&path, false).await, 0);
    tokio::time::advance(Duration::from_millis(999)).await;
    assert_eq!(pending_count(&store).await, 1);
    assert_eq!(audit_writes(&path, false).await, 0);
    tokio::time::advance(Duration::from_millis(1)).await;
    within(async { while pending_count(&store).await != 0 {} }).await;
    assert_eq!(audit_writes(&path, false).await, 1);
    for visits in 101..=200 {
        assert!(store.put(key, "model", response(visits)));
        assert_eq!(pending_count(&store).await, 1);
    }
    assert_eq!(audit_writes(&path, false).await, 1);
    within(store.shutdown()).await;
    assert_eq!(audit_writes(&path, false).await, 2);
    let reopened = cache(&directory);
    assert_eq!(
        within(reopened.get(key, "model")).await,
        Some(response(200))
    );
    within(reopened.shutdown()).await;
    clock_guard.abort();
}

#[tokio::test]
async fn equally_deep_pending_final_preserves_latest_ownership_when_flushed() {
    let directory = TempDir::new().unwrap();
    let store = cache(&directory);
    let key = AnalysisKey([8; 32]);
    let partial = response(100);
    let mut final_response = partial.clone();
    final_response.id = "latest-final".into();
    final_response.is_during_search = false;
    final_response.ownership = Some(vec![0.9, -0.8, 0.7]);
    assert!(store.put(key, "model", partial));
    assert!(store.put(key, "model", final_response.clone()));
    // An older shallower snapshot must not displace the latest final.
    assert!(store.put(key, "model", response(99)));
    assert_eq!(
        within(store.get(key, "model")).await,
        Some(final_response.clone())
    );
    within(store.flush_position(key)).await;
    let reopened = cache(&directory);
    assert_eq!(
        within(reopened.get(key, "model")).await,
        Some(final_response)
    );
    within(reopened.shutdown()).await;
    within(store.shutdown()).await;
}

#[tokio::test]
async fn equally_deep_final_replaces_already_persisted_partial_on_shutdown() {
    let directory = TempDir::new().unwrap();
    let store = cache(&directory);
    let key = AnalysisKey([9; 32]);
    let partial = response(100);
    assert!(store.put(key, "model", partial.clone()));
    within(store.flush()).await;
    let mut final_response = partial;
    final_response.id = "final-after-earlier-write".into();
    final_response.is_during_search = false;
    final_response.ownership = Some(vec![-0.9, 0.8, -0.7]);
    assert!(store.put(key, "model", final_response.clone()));
    assert_eq!(
        within(store.get(key, "model")).await,
        Some(final_response.clone())
    );
    within(store.shutdown()).await;
    let reopened = cache(&directory);
    assert_eq!(
        within(reopened.get(key, "model")).await,
        Some(final_response)
    );
    // The database still rejects later responses with fewer visits.
    assert!(reopened.put(key, "model", response(99)));
    within(reopened.flush()).await;
    assert_eq!(
        within(reopened.get(key, "model"))
            .await
            .unwrap()
            .root_info
            .visits,
        100
    );
    within(reopened.shutdown()).await;
}

#[tokio::test]
async fn put_returns_while_database_is_held_and_saturation_drops_without_waiting() {
    let directory = TempDir::new().unwrap();
    let store = cache(&directory);
    let key = AnalysisKey([2; 32]);
    assert_eq!(within(store.get(key, "model")).await, None);
    let (entered, entered_rx) = oneshot::channel();
    let (release, release_rx) = oneshot::channel();
    let (finished, mut finished_rx) = oneshot::channel();
    store
        .commands
        .send(Command::HoldDatabase {
            operation: Operation::Get(key, "model".into()),
            entered,
            release: release_rx,
            finished,
        })
        .await
        .unwrap();
    within(entered_rx).await.unwrap();
    // The worker owns SQLite and cannot finish before release below. These
    // calls must still return, including the overflow path, with no await.
    let producer = store.clone();
    within(tokio::task::spawn_blocking(move || {
        for visits in 1..=256 {
            assert!(producer.put(key, "model", response(visits)));
        }
        assert!(!producer.put(key, "model", response(257)));
    }))
    .await
    .unwrap();
    assert!(matches!(
        finished_rx.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    release.send(()).unwrap();
    within(finished_rx).await.unwrap();
    within(store.shutdown()).await;
    let reopened = cache(&directory);
    assert_eq!(
        within(reopened.get(key, "model")).await,
        Some(response(256))
    );
    within(reopened.shutdown()).await;
}

#[tokio::test]
async fn position_flush_preserves_final_snapshot_for_all_engines() {
    let directory = TempDir::new().unwrap();
    let store = cache(&directory);
    let key = AnalysisKey([3; 32]);
    for visits in 1..=100 {
        assert!(store.put(key, "model-a", response(visits)));
        assert!(store.put(key, "model-b", response(visits)));
    }
    within(store.flush_position(key)).await;
    let reopened = cache(&directory);
    for engine in ["model-a", "model-b"] {
        assert_eq!(within(reopened.get(key, engine)).await, Some(response(100)));
    }
    within(reopened.shutdown()).await;
    within(store.shutdown()).await;
}

#[tokio::test]
async fn disabling_flushes_accepted_writes_and_ignores_subsequent_snapshots() {
    let directory = TempDir::new().unwrap();
    let store = cache(&directory);
    let key = AnalysisKey([4; 32]);
    assert!(store.put(key, "model", response(100)));
    within(store.set_enabled(false)).await;
    assert!(store.put(key, "model", response(200)));
    assert_eq!(pending_count(&store).await, 0);
    within(store.set_enabled(true)).await;
    assert_eq!(within(store.get(key, "model")).await, Some(response(100)));
    within(store.shutdown()).await;
    let reopened = cache(&directory);
    assert_eq!(
        within(reopened.get(key, "model")).await,
        Some(response(100))
    );
    within(reopened.shutdown()).await;
}

#[tokio::test]
async fn dropping_last_handle_flushes_queued_and_pending_snapshots() {
    let directory = TempDir::new().unwrap();
    let store = cache(&directory);
    let key = AnalysisKey([5; 32]);
    let mut status = store.status();
    assert!(store.put(key, "model", response(100)));
    assert_eq!(pending_count(&store).await, 1);
    assert!(store.put(key, "model", response(200)));
    drop(store);
    within(status.wait_for(|status| *status == CacheStatus::Stopped))
        .await
        .unwrap();
    let reopened = cache(&directory);
    assert_eq!(
        within(reopened.get(key, "model")).await,
        Some(response(200))
    );
    within(reopened.shutdown()).await;
}

#[tokio::test]
async fn clear_discards_pending_writes_without_resurrecting_them_at_shutdown() {
    let directory = TempDir::new().unwrap();
    let store = cache(&directory);
    let key = AnalysisKey([6; 32]);
    let other = AnalysisKey([7; 32]);
    assert!(store.put(key, "model-a", response(100)));
    assert!(store.put(key, "model-b", response(100)));
    assert!(store.put(other, "model", response(100)));
    within(store.clear_position(key)).await;
    assert_eq!(within(store.get(key, "model-a")).await, None);
    assert_eq!(within(store.get(key, "model-b")).await, None);
    assert_eq!(within(store.get(other, "model")).await, Some(response(100)));
    within(store.flush()).await;
    assert!(store.put(other, "model", response(200)));
    within(store.clear_all()).await;
    within(store.shutdown()).await;
    let reopened = cache(&directory);
    for (key, engine) in [(key, "model-a"), (key, "model-b"), (other, "model")] {
        assert_eq!(within(reopened.get(key, engine)).await, None);
    }
    within(reopened.shutdown()).await;
}

#[tokio::test]
async fn pending_keys_are_bounded_and_oldest_is_flushed_on_overflow() {
    let directory = TempDir::new().unwrap();
    let store = cache(&directory);
    for index in 0..=actor::MAX_PENDING {
        let key = AnalysisKey([u8::try_from(index).unwrap(); 32]);
        assert!(store.put(key, "model", response(100)));
        assert!(pending_count(&store).await <= actor::MAX_PENDING);
    }
    // The first item is persisted by overflow, before shutdown flushes the rest.
    let reopened = cache(&directory);
    assert_eq!(
        within(reopened.get(AnalysisKey([0; 32]), "model")).await,
        Some(response(100))
    );
    within(reopened.shutdown()).await;
    within(store.shutdown()).await;
}
