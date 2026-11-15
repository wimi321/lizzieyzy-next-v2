#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::remember_state;
use app_core::{AppEvent, CoachSummary, CoachView, ColorView};
use std::{future::Future, time::Duration};
use tauri::ipc::Channel;
use tokio::sync::mpsc;

#[test]
fn new_coach_session_clears_old_summary_and_keeps_current_view_in_publication_order() {
    let summary = CoachSummary::from_ratings(None, None, 32, 2, 1, &[]);
    let mut replay = vec![];
    remember_state(&mut replay, &AppEvent::CoachSummary(summary.clone()));
    let mut view = CoachView {
        active: true,
        setup: Default::default(),
        human_color: ColorView::Black,
        awaiting_decision: false,
        hints: 2,
        take_backs: 1,
        last_rating: None,
        summary: Some(summary),
    };
    remember_state(&mut replay, &AppEvent::CoachUpdated(view.clone()));
    assert_eq!(replay.len(), 2);
    assert!(matches!(replay[0], AppEvent::CoachSummary(_)));
    view.summary = None;
    view.hints = 0;
    view.take_backs = 0;
    remember_state(&mut replay, &AppEvent::CoachUpdated(view.clone()));
    assert_eq!(replay.len(), 1);
    assert!(matches!(&replay[0], AppEvent::CoachUpdated(current) if current == &view));
}

#[test]
fn commentary_reconnect_keeps_the_latest_full_answer_and_correction_state() {
    let mut replay = vec![];
    let mut update = app_core::CommentaryUpdated {
        request_id: 8,
        phase: app_core::CommentaryPhase::Generating,
        text: "first draft".into(),
        delta: "draft".into(),
        ..Default::default()
    };
    remember_state(&mut replay, &AppEvent::CommentaryUpdated(update.clone()));
    update.text = "corrected answer".into();
    update.delta = "answer".into();
    update.replacement = true;
    remember_state(&mut replay, &AppEvent::CommentaryUpdated(update.clone()));
    assert_eq!(replay.len(), 1);
    assert!(matches!(&replay[0], AppEvent::CommentaryUpdated(current) if current == &update));
    update.phase = app_core::CommentaryPhase::Completed;
    update.delta.clear();
    remember_state(&mut replay, &AppEvent::CommentaryUpdated(update.clone()));
    assert_eq!(replay.len(), 1);
    assert!(matches!(&replay[0], AppEvent::CommentaryUpdated(current) if current == &update));
}

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("event bridge operation timed out")
}

async fn next_document(receiver: &mut mpsc::UnboundedReceiver<AppEvent>, revision: u64) {
    within(async {
        loop {
            let event = receiver.recv().await.expect("channel disconnected");
            if matches!(event, AppEvent::DocumentChanged { revision: actual, .. } if actual == revision) {
                return;
            }
        }
    }).await;
}

#[tokio::test]
async fn failed_floating_channel_does_not_disconnect_the_other_windows() {
    let directory = tempfile::TempDir::new().unwrap();
    let (core, subscription) = within(app_core::spawn(directory.path())).await.unwrap();
    let bridge = super::EventBridge::new(subscription);
    let (main_send, mut main_recv) = mpsc::unbounded_channel();
    let main = Channel::new(move |body| {
        main_send.send(body.deserialize().unwrap()).unwrap();
        Ok(())
    });
    let main_id = within(bridge.subscribe("main".into(), main)).await.unwrap();
    next_document(&mut main_recv, 0).await;
    let (float_send, mut float_recv) = mpsc::unbounded_channel();
    let floating = Channel::new(move |body| {
        let event: AppEvent = body.deserialize().unwrap();
        if matches!(event, AppEvent::DocumentChanged { revision: 1, .. }) {
            return Err(tauri::Error::AssetNotFound("closed webview".into()));
        }
        float_send.send(event).unwrap();
        Ok(())
    });
    within(bridge.subscribe("float-main".into(), floating))
        .await
        .unwrap();
    next_document(&mut float_recv, 0).await;
    let black: app_core::MoveView = serde_json::from_value(serde_json::json!({
        "type": "Pass", "data": {"color": "Black"}
    }))
    .unwrap();
    within(core.play(black.into())).await.unwrap();
    next_document(&mut main_recv, 1).await;
    within(core.step(-1)).await.unwrap();
    next_document(&mut main_recv, 2).await;
    within(bridge.unsubscribe("main".into(), main_id))
        .await
        .unwrap();
    while let Ok(event) = float_recv.try_recv() {
        assert!(!matches!(
            event,
            AppEvent::DocumentChanged {
                revision: 1 | 2,
                ..
            }
        ));
    }
    within(core.shutdown()).await.unwrap();
}
