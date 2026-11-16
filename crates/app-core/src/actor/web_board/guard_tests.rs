#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::Client;
use crate::{
    WebClientMessage,
    actor::{Actor, Input},
    test_support::within,
};
use std::{collections::VecDeque, time::Duration};
use tokio::sync::watch;

fn connected_actor() -> (
    Actor,
    watch::Receiver<crate::web_board::connection::Outbound>,
) {
    let (mut actor, _) = crate::actor::tests::actor();
    let (outgoing, receiver) = watch::channel(Default::default());
    actor.web_board.clients.insert(
        1,
        Client {
            id: "server-assigned-test-client".into(),
            reconnect: "server-assigned-test-secret".into(),
            outgoing,
            messages: VecDeque::new(),
        },
    );
    (actor, receiver)
}

#[tokio::test(start_paused = true)]
async fn rolling_trial_rate_limit_drops_excess_without_deferred_moves() {
    // This test has neither sockets nor external processes: paused time tests
    // the actual actor's rolling-window admission and document mutation.
    let (mut actor, _messages) = connected_actor();
    within(actor.web_client_message(1, WebClientMessage::EnterTrial)).await;
    assert!(actor.web_board.trial.is_some());
    for x in 0..19 {
        within(actor.web_client_message(1, WebClientMessage::TrialMove { x, y: 0 })).await;
    }
    assert_eq!(
        actor
            .document
            .position_at(actor.document.current())
            .unwrap()
            .moves
            .len(),
        9
    );
    within(tokio::time::advance(Duration::from_millis(999))).await;
    within(actor.web_client_message(1, WebClientMessage::TrialMove { x: 10, y: 1 })).await;
    assert_eq!(
        actor
            .document
            .position_at(actor.document.current())
            .unwrap()
            .moves
            .len(),
        9
    );
    within(tokio::time::advance(Duration::from_millis(1))).await;
    within(actor.web_client_message(1, WebClientMessage::TrialMove { x: 10, y: 1 })).await;
    assert_eq!(
        actor
            .document
            .position_at(actor.document.current())
            .unwrap()
            .moves
            .len(),
        10
    );
    // Messages discarded in the first window are never replayed later.
    assert!(
        actor
            .document
            .position_at(actor.document.current())
            .unwrap()
            .board
            .stone_at(go_core::Point { x: 18, y: 0 })
            .is_none()
    );
    within(actor.apply(Input::Shutdown)).await.unwrap();
}

#[tokio::test]
async fn running_or_paused_review_and_batch_reject_remote_trial_without_cancelling_work() {
    for phase in [
        crate::WholeGameStatus::Running,
        crate::WholeGameStatus::Paused,
    ] {
        let (mut actor, _messages) = connected_actor();
        let progress = crate::WholeGameProgress {
            source: crate::WholeGameSource::Manual,
            completed: 2,
            total: 5,
            visits: 100,
            status: phase,
        };
        actor.review.progress = Some(progress.clone());
        within(actor.web_client_message(1, WebClientMessage::EnterTrial)).await;
        assert!(actor.web_board.trial.is_none());
        assert!(actor.move_trial.is_none());
        assert_eq!(actor.review.progress, Some(progress));
        within(actor.apply(Input::Shutdown)).await.unwrap();
    }
    for phase in [crate::BatchPhase::Running, crate::BatchPhase::Paused] {
        let (mut actor, _messages) = connected_actor();
        actor.batch.status.phase = phase;
        within(actor.web_client_message(1, WebClientMessage::EnterTrial)).await;
        assert!(actor.web_board.trial.is_none());
        assert!(actor.move_trial.is_none());
        assert_eq!(actor.batch.status.phase, phase);
        within(actor.apply(Input::Shutdown)).await.unwrap();
    }
}
