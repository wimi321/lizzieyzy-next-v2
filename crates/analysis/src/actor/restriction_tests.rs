#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::tests::{actor, flush_pending, immediately_available, snapshot, stop};
use super::*;
use crate::{AnalysisRestriction, RestrictionKind, SearchParams};
use go_core::Point;
use std::{future::Future, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("restriction safety deadline")
}

#[tokio::test]
async fn restricted_results_bypass_lru_and_external_seed_and_clear_restores_original() {
    let (mut actor, mut output) = actor();
    actor.config.live.max_visits = 1;
    let old = snapshot(&actor, 1000, SnapshotSource::Cache);
    actor.cache.insert(
        old.key,
        &actor.config.engine_fingerprint,
        old.response.clone(),
    );
    let restriction = AnalysisRestriction {
        kind: RestrictionKind::Allow,
        points: vec![Point { x: 3, y: 3 }],
        depth: 5,
    };
    within(actor.command(Input::SetRestriction(Some(restriction))))
        .await
        .unwrap();
    assert!(actor.last_published.is_none());
    let focus = actor.focus.clone().unwrap();
    let poisoned = snapshot(&actor, 9999, SnapshotSource::Cache).response;
    within(actor.command(Input::Focus(
        Box::new(focus),
        Some(Box::new(poisoned)),
        false,
    )))
    .await
    .unwrap();
    let event = within(receive(&mut actor.active)).await;
    assert!(matches!(event, Some(QueryEvent::Final(_))));
    actor.event(event);
    flush_pending(&mut actor);
    let result = within(immediately_available(&mut output)).await;
    assert_eq!(result.source, SnapshotSource::Restricted);
    assert_eq!(result.response.move_infos[0].coordinate, "D16");
    assert_eq!(
        actor.cache.get(old.key, &actor.config.engine_fingerprint),
        Some(old.response.clone())
    );
    within(actor.command(Input::SetRestriction(None)))
        .await
        .unwrap();
    let restored = within(immediately_available(&mut output)).await;
    assert_eq!(restored.source, SnapshotSource::Cache);
    assert_eq!(restored.response, old.response);
    within(actor.command(Input::SetPondering(false)))
        .await
        .unwrap();
    within(stop(actor)).await;
}

#[tokio::test]
async fn invalid_restriction_keeps_live_subscription_and_search_identity_is_immutable() {
    let (mut actor, _) = actor();
    within(actor.start()).await.unwrap();
    let id = actor.active.as_ref().unwrap().subscription.id().to_owned();
    let invalid = AnalysisRestriction {
        kind: RestrictionKind::Avoid,
        points: vec![Point { x: 99, y: 0 }],
        depth: 5,
    };
    assert!(
        within(actor.command(Input::SetRestriction(Some(invalid))))
            .await
            .is_err()
    );
    assert_eq!(actor.active.as_ref().unwrap().subscription.id(), id);
    let live = LiveSettings {
        search_params: Some(SearchParams {
            wide_root_noise: Some(0.5),
            ..Default::default()
        }),
        ..actor.config.live.clone()
    };
    assert!(matches!(
        within(actor.command(Input::SetLiveSettings(live))).await,
        Err(AnalysisError::InvalidSettings(_))
    ));
    assert_eq!(actor.active.as_ref().unwrap().subscription.id(), id);
    within(actor.command(Input::SetPondering(false)))
        .await
        .unwrap();
    within(stop(actor)).await;
}

#[tokio::test]
async fn pausing_after_restricted_extra_completion_preserves_unread_payload() {
    let (mut actor, mut output) = actor();
    actor.config.restriction = Some(AnalysisRestriction {
        kind: RestrictionKind::Allow,
        points: vec![Point { x: 3, y: 3 }],
        depth: 5,
    });
    within(
        actor.command(Input::SetAnalysisExtras(crate::AnalysisExtras {
            policy: true,
            moves_ownership: true,
        })),
    )
    .await
    .unwrap();
    let event = within(receive(&mut actor.active)).await;
    assert!(matches!(event, Some(QueryEvent::Final(_))));
    actor.event(event);
    flush_pending(&mut actor);
    assert_eq!(
        actor.last_published.as_ref().unwrap().source,
        SnapshotSource::Restricted
    );
    // Resume live just as the coordinator run loop does after a supplement,
    // then pause before the application has consumed the completed payload.
    within(actor.start()).await.unwrap();
    within(actor.command(Input::SetPondering(false)))
        .await
        .unwrap();
    flush_pending(&mut actor);
    let result = within(immediately_available(&mut output)).await;
    assert_eq!(result.source, SnapshotSource::Restricted);
    assert!(result.response.policy.is_some());
    assert!(!result.response.move_infos.is_empty());
    assert!(!actor.config.pondering);
    assert!(actor.active.is_none());
    within(stop(actor)).await;
}
