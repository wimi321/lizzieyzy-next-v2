#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::tests::{actor, assert_no_output, flush_pending, immediately_available, snapshot, stop};
use super::*;
use crate::AnalysisExtras;
use document::GameDocument;
use go_core::{Color, Move, Point};
use std::time::Duration;
use tokio::time::timeout;

async fn next(actor: &mut Actor) {
    let event = timeout(Duration::from_secs(5), receive(&mut actor.active))
        .await
        .expect("fake response safety deadline");
    assert!(event.is_some());
    assert_eq!(actor.event(event), None);
}

async fn finish_one_shot(actor: &mut Actor) {
    while actor.active.is_some() {
        next(actor).await;
    }
    flush_pending(actor);
}

fn extras() -> AnalysisExtras {
    AnalysisExtras {
        policy: true,
        moves_ownership: true,
    }
}

fn next_focus(actor: &Actor) -> Focus {
    let mut document = GameDocument::new();
    document
        .apply(document::Command::Play(Move::Play(
            Color::Black,
            Point { x: 3, y: 3 },
        )))
        .unwrap();
    let position = document.position_at(document.current()).unwrap();
    Focus {
        revision: actor.focus.as_ref().unwrap().revision + 1,
        node_id: document.current(),
        key: position.analysis_key(),
        position,
    }
}

#[tokio::test]
async fn paused_extras_fetch_one_visit_and_keep_pause_intent_and_complete_shape() {
    let (mut actor, mut output) = actor();
    actor.config.pondering = false;
    actor.focus = Some(next_focus(&actor));
    actor
        .command(Input::SetAnalysisExtras(extras()))
        .await
        .unwrap();
    assert_eq!(actor.active.as_ref().unwrap().kind, QueryKind::Extras);
    finish_one_shot(&mut actor).await;
    let result = immediately_available(&mut output).await;
    assert_eq!(result.source, SnapshotSource::Extras);
    assert_eq!(result.response.root_info.visits, 1);
    assert_eq!(result.response.policy.as_ref().unwrap().len(), 362);
    assert_eq!(result.response.policy.as_ref().unwrap()[3 * 19 + 3], -1.0);
    assert!(!result.response.move_infos.is_empty());
    assert!(result.response.move_infos.iter().all(|candidate| {
        candidate
            .ownership
            .as_ref()
            .is_some_and(|ownership| ownership.len() == 361)
    }));
    assert!(!actor.config.pondering);
    assert_eq!(*actor.status.borrow(), AnalysisStatus::Idle);
    actor.start().await.unwrap();
    assert!(actor.active.is_none());
    stop(actor).await;
}

#[tokio::test]
async fn live_extras_replace_old_query_and_disable_omits_optional_payloads() {
    let (mut actor, mut output) = actor();
    actor.start().await.unwrap();
    next(&mut actor).await; // The single fake search slot is now occupied.
    let old_id = actor.active.as_ref().unwrap().subscription.id().to_owned();
    actor
        .command(Input::SetAnalysisExtras(extras()))
        .await
        .unwrap();
    let extra_id = actor.active.as_ref().unwrap().subscription.id().to_owned();
    assert_ne!(old_id, extra_id);
    // This can finish on the same single search slot only after cancellation.
    finish_one_shot(&mut actor).await;
    let result = immediately_available(&mut output).await;
    assert_eq!(result.source, SnapshotSource::Extras);
    assert!(result.response.policy.is_some());
    assert!(
        result
            .response
            .move_infos
            .iter()
            .all(|candidate| candidate.ownership.is_some())
    );
    assert!(actor.config.pondering);
    actor.start().await.unwrap();
    assert_eq!(actor.active.as_ref().unwrap().kind, QueryKind::Live);
    next(&mut actor).await;
    actor
        .command(Input::SetAnalysisExtras(AnalysisExtras::default()))
        .await
        .unwrap();
    assert_eq!(actor.active.as_ref().unwrap().kind, QueryKind::Live);
    // Inspect the actual new subscription independently of the display floor.
    let event = timeout(Duration::from_secs(5), receive(&mut actor.active))
        .await
        .unwrap()
        .unwrap();
    let response = match event {
        QueryEvent::Partial(response) | QueryEvent::Final(response) => response,
        other => panic!("{other:?}"),
    };
    assert!(response.policy.is_none());
    assert!(
        response
            .move_infos
            .iter()
            .all(|candidate| candidate.ownership.is_none())
    );
    stop(actor).await;
}

#[tokio::test]
async fn old_cache_extras_are_merged_without_losing_depth_or_resuming_limits() {
    let (mut actor, mut output) = actor();
    actor.config.pondering = false;
    let old = snapshot(&actor, 5000, SnapshotSource::Cache);
    let key = old.key;
    actor
        .cache
        .insert(key, &actor.config.engine_fingerprint, old.response.clone());
    actor.offer_cached();
    immediately_available(&mut output).await;
    actor.limited = Some(LimitReason::Time);
    actor.limited_on_current = true;
    actor
        .command(Input::SetAnalysisExtras(extras()))
        .await
        .unwrap();
    finish_one_shot(&mut actor).await;
    let result = immediately_available(&mut output).await;
    assert_eq!(result.source, SnapshotSource::Extras);
    assert_eq!(result.response.root_info, old.response.root_info);
    assert!(result.response.policy.is_some());
    assert_eq!(actor.limited, Some(LimitReason::Time));
    assert_eq!(
        *actor.status.borrow(),
        AnalysisStatus::Limited(LimitReason::Time)
    );
    assert!(actor.limited_on_current);
    assert_eq!(
        actor.cache.get(key, &actor.config.engine_fingerprint),
        Some(old.response)
    );
    actor.start().await.unwrap();
    assert!(actor.active.is_none());
    stop(actor).await;
}

#[tokio::test]
async fn completed_live_search_only_fetches_extras_once_without_restarting_search() {
    let (mut actor, mut output) = actor();
    actor.config.live.max_visits = 1;
    actor.start().await.unwrap();
    finish_one_shot(&mut actor).await;
    immediately_available(&mut output).await;
    assert!(actor.live_finished);
    actor
        .command(Input::SetAnalysisExtras(extras()))
        .await
        .unwrap();
    finish_one_shot(&mut actor).await;
    assert_eq!(
        immediately_available(&mut output).await.source,
        SnapshotSource::Extras
    );
    actor.start().await.unwrap();
    assert!(actor.active.is_none());
    assert!(actor.config.pondering);
    stop(actor).await;
}

#[tokio::test]
async fn pure_network_uses_one_visit_replaces_deep_display_and_restores_pause() {
    let (mut actor, mut output) = actor();
    actor.config.pondering = false;
    let old = snapshot(&actor, 5000, SnapshotSource::Live);
    actor.offer(old.clone());
    immediately_available(&mut output).await;
    actor.command(Input::SetPureNetwork(true)).await.unwrap();
    finish_one_shot(&mut actor).await;
    let result = immediately_available(&mut output).await;
    assert_eq!(result.source, SnapshotSource::PureNetwork);
    assert_eq!(result.response.root_info.visits, 1);
    assert_eq!(result.response.policy.as_ref().unwrap().len(), 362);
    assert_eq!(result.response.ownership.as_ref().unwrap().len(), 361);
    assert!(
        actor
            .cache
            .get(old.key, &actor.config.engine_fingerprint)
            .is_none()
    );
    actor.command(Input::SetPureNetwork(false)).await.unwrap();
    let restored = immediately_available(&mut output).await;
    assert_eq!(restored.response, old.response);
    assert_eq!(restored.source, SnapshotSource::Cache);
    assert!(!actor.config.pondering);
    assert!(actor.active.is_none());
    stop(actor).await;
}

#[tokio::test]
async fn pure_network_replaces_live_and_navigation_invalidates_unread_old_result() {
    let (mut actor, mut output) = actor();
    actor.start().await.unwrap();
    next(&mut actor).await;
    actor.command(Input::SetPureNetwork(true)).await.unwrap();
    finish_one_shot(&mut actor).await;
    // Leave the first pure result unread; focus success must invalidate it.
    let focus = next_focus(&actor);
    actor
        .command(Input::Focus(Box::new(focus.clone()), None, true))
        .await
        .unwrap();
    assert_no_output(&mut output).await;
    finish_one_shot(&mut actor).await;
    let result = immediately_available(&mut output).await;
    assert_eq!(
        (result.node_id, result.revision, result.key),
        (focus.node_id, focus.revision, focus.key)
    );
    assert_eq!(result.source, SnapshotSource::PureNetwork);
    assert_eq!(result.response.root_info.visits, 1);
    assert_eq!(result.response.policy.as_ref().unwrap()[3 * 19 + 3], -1.0);
    actor.command(Input::SetPureNetwork(false)).await.unwrap();
    assert_eq!(actor.active.as_ref().unwrap().kind, QueryKind::Live);
    assert!(actor.config.pondering);
    stop(actor).await;
}

#[tokio::test]
async fn switching_extra_query_to_pure_then_back_never_keeps_two_subscriptions() {
    let (mut actor, mut output) = actor();
    actor.config.pondering = false;
    actor
        .command(Input::SetAnalysisExtras(extras()))
        .await
        .unwrap();
    let extra_id = actor.active.as_ref().unwrap().subscription.id().to_owned();
    actor.command(Input::SetPureNetwork(true)).await.unwrap();
    assert_ne!(actor.active.as_ref().unwrap().subscription.id(), extra_id);
    assert_eq!(actor.active.as_ref().unwrap().kind, QueryKind::PureNetwork);
    finish_one_shot(&mut actor).await;
    assert_eq!(
        immediately_available(&mut output).await.source,
        SnapshotSource::PureNetwork
    );
    actor.command(Input::SetPureNetwork(false)).await.unwrap();
    assert_eq!(actor.active.as_ref().unwrap().kind, QueryKind::Extras);
    finish_one_shot(&mut actor).await;
    assert_eq!(
        immediately_available(&mut output).await.source,
        SnapshotSource::Extras
    );
    assert!(!actor.config.pondering);
    stop(actor).await;
}

#[tokio::test]
async fn pure_navigation_preserves_limit_pause_and_ignores_automatic_resume() {
    let (mut actor, mut output) = actor();
    actor.limited = Some(LimitReason::Playouts);
    actor.limited_on_current = true;
    actor.command(Input::SetPureNetwork(true)).await.unwrap();
    let focus = next_focus(&actor);
    actor
        .command(Input::Focus(Box::new(focus), None, true))
        .await
        .unwrap();
    finish_one_shot(&mut actor).await;
    immediately_available(&mut output).await;
    assert_eq!(actor.limited, Some(LimitReason::Playouts));
    actor.command(Input::SetPureNetwork(false)).await.unwrap();
    assert!(actor.active.is_none());
    assert_eq!(
        *actor.status.borrow(),
        AnalysisStatus::Limited(LimitReason::Playouts)
    );
    stop(actor).await;
}

#[tokio::test]
async fn clearing_illegal_focus_cancels_pure_and_extras_and_invalidates_unread_output() {
    for pure in [false, true] {
        let (mut actor, mut output) = actor();
        actor.config.pondering = false;
        actor.limited = Some(LimitReason::Time);
        if pure {
            actor.command(Input::SetPureNetwork(true)).await.unwrap();
        } else {
            actor
                .command(Input::SetAnalysisExtras(extras()))
                .await
                .unwrap();
        }
        let old_focus = actor.focus.clone().unwrap();
        // First clear while the actual query subscription is outstanding.
        actor.command(Input::ClearFocus).await.unwrap();
        assert!(actor.active.is_none());
        assert!(actor.focus.is_none());
        assert_no_output(&mut output).await;
        assert_eq!(actor.config.pure_network, pure);
        assert_eq!(actor.limited, Some(LimitReason::Time));
        assert!(!actor.config.pondering);
        actor
            .command(Input::Focus(Box::new(old_focus), None, false))
            .await
            .unwrap();
        finish_one_shot(&mut actor).await;
        // Clear again with a completed but unread one-shot result.
        actor.command(Input::ClearFocus).await.unwrap();
        assert_no_output(&mut output).await;
        assert!(actor.throttle.deadline().is_none());
        stop(actor).await;
    }
}

#[tokio::test]
async fn pure_network_prefers_explicit_raw_evaluation_without_changing_policy() {
    let (mut actor, mut output) = actor();
    actor.config.pondering = false;
    actor.command(Input::SetPureNetwork(true)).await.unwrap();
    let event = timeout(Duration::from_secs(5), receive(&mut actor.active))
        .await
        .unwrap()
        .unwrap();
    let mut response = match event {
        QueryEvent::Final(response) => response,
        other => panic!("{other:?}"),
    };
    response.root_info.raw_winrate = Some(0.23);
    response.root_info.raw_lead = Some(-4.25);
    response.root_info.winrate = 0.4;
    response.root_info.score_lead = -2.0;
    let policy = response.policy.clone();
    actor.event(Some(QueryEvent::Final(response)));
    flush_pending(&mut actor);
    let result = immediately_available(&mut output).await;
    assert_eq!(result.response.root_info.winrate, 0.23);
    assert_eq!(result.response.root_info.score_lead, -4.25);
    assert_eq!(result.response.policy, policy);
    stop(actor).await;
}

#[tokio::test]
async fn pausing_during_supplement_replaces_it_without_losing_requested_payload() {
    let (mut actor, mut output) = actor();
    actor
        .command(Input::SetAnalysisExtras(extras()))
        .await
        .unwrap();
    let previous = actor.active.as_ref().unwrap().subscription.id().to_owned();
    actor.command(Input::SetPondering(false)).await.unwrap();
    let active = actor.active.as_ref().unwrap();
    assert_eq!(active.kind, QueryKind::Extras);
    assert_ne!(active.subscription.id(), previous);
    finish_one_shot(&mut actor).await;
    let result = immediately_available(&mut output).await;
    assert_eq!(result.source, SnapshotSource::Extras);
    assert!(result.response.policy.is_some());
    assert!(!actor.config.pondering);
    actor.start().await.unwrap();
    assert!(actor.active.is_none());
    stop(actor).await;
}

#[tokio::test]
async fn pausing_after_throttled_supplement_keeps_payload_when_live_has_already_resumed() {
    let (mut actor, mut output) = actor();
    let old = snapshot(&actor, 5000, SnapshotSource::Live);
    actor.offer(old);
    immediately_available(&mut output).await;
    actor
        .command(Input::SetAnalysisExtras(extras()))
        .await
        .unwrap();
    let response = timeout(Duration::from_secs(5), receive(&mut actor.active))
        .await
        .unwrap()
        .unwrap();
    // Pin the throttle window in virtual time, independently of process I/O.
    tokio::time::pause();
    actor.throttle.rebase_revision(1, Instant::now(), true);
    actor.event(Some(response));
    assert!(actor.throttle.deadline().is_some());
    assert!(
        actor
            .last_published
            .as_ref()
            .unwrap()
            .response
            .policy
            .is_none()
    );
    tokio::time::resume();
    actor.start().await.unwrap();
    assert_eq!(actor.active.as_ref().unwrap().kind, QueryKind::Live);
    actor.command(Input::SetPondering(false)).await.unwrap();
    tokio::time::pause();
    flush_pending(&mut actor);
    let available = tokio::select! {
        biased;
        result = output.recv() => result,
        _ = std::future::ready(()) => None,
    };
    tokio::time::resume();
    stop(actor).await;
    let result = available.expect("completed supplementary payload must survive pausing");
    assert_eq!(result.source, SnapshotSource::Extras);
    assert!(result.response.policy.is_some());
    assert_eq!(result.response.root_info.visits, 5000);
}

#[tokio::test]
async fn same_focus_pause_republishes_unread_extras_and_pure_final() {
    for pure in [false, true] {
        let (mut actor, mut output) = actor();
        if pure {
            actor.command(Input::SetPureNetwork(true)).await.unwrap();
        } else {
            actor
                .command(Input::SetAnalysisExtras(extras()))
                .await
                .unwrap();
        }
        finish_one_shot(&mut actor).await;
        // The coordinator published it, but AppCore has not consumed it yet.
        actor.command(Input::SetPondering(false)).await.unwrap();
        tokio::time::pause();
        flush_pending(&mut actor);
        let available = tokio::select! {
            biased;
            result = output.recv() => result,
            _ = std::future::ready(()) => None,
        };
        tokio::time::resume();
        assert!(actor.active.is_none());
        stop(actor).await;
        let result = available.expect("unread transient final must survive pausing");
        assert_eq!(
            result.source,
            if pure {
                SnapshotSource::PureNetwork
            } else {
                SnapshotSource::Extras
            }
        );
        assert!(result.response.policy.is_some());
    }
}

#[tokio::test]
async fn disabling_one_extra_republishes_unread_complete_payload_without_query() {
    let (mut actor, mut output) = actor();
    actor.config.pondering = false;
    actor
        .command(Input::SetAnalysisExtras(extras()))
        .await
        .unwrap();
    finish_one_shot(&mut actor).await;
    actor
        .command(Input::SetAnalysisExtras(AnalysisExtras {
            policy: true,
            moves_ownership: false,
        }))
        .await
        .unwrap();
    tokio::time::pause();
    flush_pending(&mut actor);
    let available = tokio::select! {
        biased;
        result = output.recv() => result,
        _ = std::future::ready(()) => None,
    };
    tokio::time::resume();
    assert!(actor.active.is_none());
    stop(actor).await;
    assert!(
        available
            .expect("complete payload must survive reducing extras")
            .response
            .policy
            .is_some()
    );
}

#[tokio::test]
async fn completed_supplement_missing_optional_fields_does_not_loop_on_pause() {
    let (mut actor, mut output) = actor();
    actor
        .command(Input::SetAnalysisExtras(extras()))
        .await
        .unwrap();
    let event = timeout(Duration::from_secs(5), receive(&mut actor.active))
        .await
        .unwrap()
        .unwrap();
    let mut response = match event {
        QueryEvent::Final(response) => response,
        other => panic!("{other:?}"),
    };
    response.policy = None;
    for candidate in &mut response.move_infos {
        candidate.ownership = None;
    }
    actor.event(Some(QueryEvent::Final(response)));
    flush_pending(&mut actor);
    actor.start().await.unwrap();
    assert_eq!(actor.active.as_ref().unwrap().kind, QueryKind::Live);
    actor.command(Input::SetPondering(false)).await.unwrap();
    flush_pending(&mut actor);
    for _ in 0..3 {
        actor.start().await.unwrap();
        assert!(actor.active.is_none());
    }
    let result = immediately_available(&mut output).await;
    assert!(result.response.policy.is_none());
    stop(actor).await;
}
