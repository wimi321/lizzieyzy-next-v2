#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{AnalysisRequest, EngineStatus, QueryEvent, SessionConfig, TransportSpec, spawn};
use std::{collections::BTreeMap, time::Duration};
use tokio::time::timeout;

#[tokio::test]
async fn mailbox_has_one_partial_slot_and_bounded_reliable_history() {
    let engine = spawn(
        TransportSpec::LocalProcess {
            program: fake_katago::binary_path().unwrap(),
            args: ["analysis", "--threads", "1"].map(Into::into).into(),
            cwd: None,
            env: BTreeMap::new(),
        },
        SessionConfig::default(),
    );
    timeout(Duration::from_secs(5), async {
        let mut status = engine.status();
        status
            .wait_for(|s| matches!(s, EngineStatus::Ready { .. }))
            .await
            .unwrap();
        let request: AnalysisRequest = serde_json::from_value(serde_json::json!({
            "moves": [], "rules": "chinese", "boardXSize": 19, "boardYSize": 19,
            "maxVisits": 100, "reportDuringSearchEvery": 0.001
        }))
        .unwrap();
        let mut slow = engine.submit(request.clone()).await.unwrap();
        let mut barrier = engine.submit(request).await.unwrap();
        while !matches!(barrier.recv().await, Some(QueryEvent::Final(_))) {}
        {
            let mailbox = slow.receiver.borrow();
            assert!(
                mailbox.serial > 3,
                "multiple partials were actually emitted"
            );
            assert!(mailbox.partial.is_some());
            assert_eq!(mailbox.reliable.len(), 1);
            assert!(mailbox.closed);
        }
        engine.shutdown().await.unwrap();
        assert!(matches!(slow.recv().await, Some(QueryEvent::Partial(_))));
        assert!(matches!(slow.recv().await, Some(QueryEvent::Final(_))));
        assert!(slow.recv().await.is_none());
    })
    .await
    .unwrap();
}
