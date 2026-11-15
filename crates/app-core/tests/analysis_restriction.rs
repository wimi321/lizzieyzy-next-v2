#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "common/game.rs"]
mod support;
use app_core::{
    AnalysisRestriction, AnalysisSource, AnalysisToolsError, AnalysisView, AppError, AppEvent,
    AppSubscription, DocumentView, NewGameOptions, PointView, RestrictionDepth, RestrictionKind,
};
use common::within;
use go_core::{Color, Move, Point};
use support::{query, requests_before};

fn p(x: u8, y: u8) -> PointView {
    PointView { x, y }
}
async fn restricted(events: &mut AppSubscription, node: u32) -> DocumentView {
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::DocumentChanged { view, .. }
                    if view.tree.current == node
                        && view
                            .restricted_analysis
                            .as_ref()
                            .is_some_and(|a| !a.is_during_search) =>
                {
                    return view;
                }
                AppEvent::AnalysisUpdated(v) => {
                    panic!("restricted result leaked into normal analysis: {v:?}")
                }
                _ => {}
            }
        }
    })
    .await
}
async fn normal(events: &mut AppSubscription, node: u32) -> AnalysisView {
    within(async {
        loop {
            if let AppEvent::AnalysisUpdated(view) = events.recv().await.unwrap()
                && view.node_id == node
                && !view.is_during_search
            {
                return view;
            }
        }
    })
    .await
}
async fn state(
    events: &mut AppSubscription,
    expected: Option<&AnalysisRestriction>,
) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = events.recv().await.unwrap()
                && view.analysis_restriction.as_ref() == expected
            {
                return view;
            }
        }
    })
    .await
}

#[tokio::test]
async fn restriction_replace_add_remove_depth_persist_and_open_or_new_game_clear() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let first =
        within(app.set_analysis_restriction(RestrictionKind::Allow, vec![p(3, 3), p(3, 3)]))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(first.points, vec![p(3, 3)]);
    assert_eq!(first.depth, 5);
    assert_eq!(
        state(&mut events, Some(&first)).await.analysis_restriction,
        Some(first)
    );
    let added = within(app.add_restriction_points(RestrictionKind::Allow, vec![p(4, 4)]))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(added.points, vec![p(3, 3), p(4, 4)]);
    let replaced = within(app.add_restriction_points(RestrictionKind::Avoid, vec![p(5, 5)]))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(replaced.kind, RestrictionKind::Avoid);
    assert_eq!(replaced.points, vec![p(5, 5)]);
    within(app.set_restriction_depth(RestrictionKind::Allow, RestrictionDepth::FirstMove))
        .await
        .unwrap();
    within(app.set_restriction_depth(RestrictionKind::Avoid, RestrictionDepth::WholeSearch))
        .await
        .unwrap();
    let deep = AnalysisRestriction {
        depth: 1_000_000,
        ..replaced
    };
    state(&mut events, Some(&deep)).await;
    for depth in [0, 201] {
        assert!(matches!(
            within(
                app.set_restriction_depth(RestrictionKind::Avoid, RestrictionDepth::Custom(depth))
            )
            .await,
            Err(AppError::AnalysisTools(AnalysisToolsError::InvalidDepth))
        ));
    }
    assert!(matches!(
        within(app.set_analysis_restriction(RestrictionKind::Allow, vec![p(19, 0)])).await,
        Err(AppError::AnalysisTools(AnalysisToolsError::InvalidPoint))
    ));
    assert_eq!(
        within(app.remove_restriction_points(vec![p(5, 5)]))
            .await
            .unwrap(),
        None
    );
    let settings = within(app.settings()).await.unwrap();
    assert_eq!(
        settings.restriction_depths.allow,
        RestrictionDepth::FirstMove
    );
    assert_eq!(
        settings.restriction_depths.avoid,
        RestrictionDepth::WholeSearch
    );
    let json = std::fs::read_to_string(directory.path().join("settings.json")).unwrap();
    assert!(!json.contains("\"points\""));
    within(app.set_analysis_restriction(RestrictionKind::Allow, vec![p(1, 1)]))
        .await
        .unwrap();
    within(app.open_bytes(b"(;SZ[9])".to_vec())).await.unwrap();
    state(&mut events, None).await;
    within(app.set_analysis_restriction(RestrictionKind::Avoid, vec![p(2, 2)]))
        .await
        .unwrap();
    within(app.new_game(NewGameOptions::default()))
        .await
        .unwrap();
    state(&mut events, None).await;
    within(app.shutdown()).await.unwrap();
    let loaded = within(app_core::Settings::load(directory.path()))
        .await
        .unwrap();
    assert_eq!(loaded.restriction_depths, settings.restriction_depths);
}

#[tokio::test]
async fn restricted_queries_follow_turn_and_depth_without_storing_or_exporting_results() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = support::config(directory.path());
    config.analysis.live.max_visits = 7;
    let mut h = support::harness_in(directory, config, vec![]).await;
    within(h.app.set_cache_enabled(true)).await.unwrap();
    within(h.app.open_bytes(b"(;SZ[9])".to_vec()))
        .await
        .unwrap();
    within(h.app.set_pondering(true)).await.unwrap();
    let original = normal(&mut h.events, 0).await;
    let ordinary = query(h.directory.path(), |q| q.get("moves").is_some()).await;
    assert_eq!(original.source, AnalysisSource::Live);
    let stored = within(h.app.whole_game_analysis(0)).await.unwrap().unwrap();
    within(
        h.app
            .set_analysis_restriction(RestrictionKind::Allow, vec![p(3, 3)]),
    )
    .await
    .unwrap();
    let allowed = query(h.directory.path(), |q| q.get("allowMoves").is_some()).await;
    assert_eq!(
        allowed["allowMoves"],
        serde_json::json!([{"player":"B","moves":["D6"],"untilDepth":5}])
    );
    assert!(allowed.get("avoidMoves").is_none());
    let restricted_view = restricted(&mut h.events, 0).await;
    assert!(
        restricted_view
            .restricted_analysis
            .as_ref()
            .unwrap()
            .candidates
            .iter()
            .all(|c| c.coordinate == "D6")
    );
    assert_eq!(
        restricted_view.graph.points[0].summary,
        Some(original.summary)
    );
    assert_eq!(
        within(h.app.whole_game_analysis(0)).await.unwrap(),
        Some(stored.clone())
    );
    within(
        h.app
            .set_restriction_depth(RestrictionKind::Avoid, RestrictionDepth::FirstMove),
    )
    .await
    .unwrap();
    within(
        h.app
            .set_analysis_restriction(RestrictionKind::Avoid, vec![p(3, 3)]),
    )
    .await
    .unwrap();
    let avoided = query(h.directory.path(), |q| q.get("avoidMoves").is_some()).await;
    assert_eq!(
        avoided["avoidMoves"],
        serde_json::json!([{"player":"B","moves":["D6"],"untilDepth":1}])
    );
    assert!(avoided.get("allowMoves").is_none());
    let v = restricted(&mut h.events, 0).await;
    assert!(
        v.restricted_analysis
            .unwrap()
            .candidates
            .iter()
            .all(|c| c.coordinate != "D6")
    );
    within(h.app.play(Move::Play(Color::Black, Point { x: 0, y: 0 })))
        .await
        .unwrap();
    let white = query(h.directory.path(), |q| q["avoidMoves"][0]["player"] == "W").await;
    assert_eq!(white["avoidMoves"][0]["untilDepth"], 1);
    restricted(&mut h.events, 1).await;
    assert!(
        within(h.app.whole_game_analysis(1))
            .await
            .unwrap()
            .is_none()
    );
    let exported = h.directory.path().join("restricted.sgf");
    within(h.app.save(&exported)).await.unwrap();
    let collection = sgf::parse(&std::fs::read(&exported).unwrap()).unwrap();
    let document = document::GameDocument::from_sgf(&collection.games[0]).unwrap();
    assert_eq!(
        document
            .saved_analysis(document::NodeId(0))
            .unwrap()
            .unwrap()
            .visits,
        7
    );
    assert!(
        document
            .saved_analysis(document::NodeId(1))
            .unwrap()
            .is_none()
    );
    within(h.app.set_pondering(false)).await.unwrap();
    within(h.app.clear_analysis_restriction()).await.unwrap();
    assert_eq!(
        within(h.app.whole_game_analysis(0)).await.unwrap(),
        Some(stored)
    );
    within(h.app.go_to(0)).await.unwrap();
    let cache = normal(&mut h.events, 0).await;
    assert_eq!(cache.source, AnalysisSource::Cache);
    assert_eq!(cache.candidates, original.candidates);
    // A following ordinary request fences all restriction queries before shutdown.
    within(h.app.set_pondering(true)).await.unwrap();
    let prefix = requests_before(h.directory.path(), |q| {
        q.get("moves").is_some()
            && q["id"] != allowed["id"]
            && q["id"] != avoided["id"]
            && q["id"] != white["id"]
            && q.get("allowMoves").is_none()
            && q.get("avoidMoves").is_none()
            && q["id"] != ordinary["id"]
    })
    .await;
    assert!(!prefix.is_empty());
    within(h.app.set_pondering(false)).await.unwrap();
    within(h.app.shutdown()).await.unwrap();
    let mut cfg = support::config(h.directory.path());
    cfg.analysis.live.max_visits = 1;
    let (app, mut events) = within(app_core::spawn_with_config(h.directory.path(), cfg))
        .await
        .unwrap();
    within(app.open_path(exported)).await.unwrap();
    within(app.go_to(1)).await.unwrap();
    within(app.set_pondering(true)).await.unwrap();
    let fresh = normal(&mut events, 1).await;
    assert_eq!(fresh.source, AnalysisSource::Live);
    assert_eq!(
        fresh.visits, 1,
        "restricted node never entered persistent cache"
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn running_and_paused_human_games_reject_restrictions_with_typed_error() {
    let h = support::harness(vec![]).await;
    within(h.app.start_game(support::setup())).await.unwrap();
    for paused in [false, true] {
        if paused {
            within(h.app.pause_game()).await.unwrap();
        }
        assert!(matches!(
            within(
                h.app
                    .set_analysis_restriction(RestrictionKind::Allow, vec![p(3, 3)])
            )
            .await,
            Err(AppError::AnalysisTools(AnalysisToolsError::GameActive))
        ));
        assert!(matches!(
            within(
                h.app
                    .add_restriction_points(RestrictionKind::Avoid, vec![p(3, 3)])
            )
            .await,
            Err(AppError::AnalysisTools(AnalysisToolsError::GameActive))
        ));
        assert!(matches!(
            within(
                h.app
                    .set_restriction_depth(RestrictionKind::Allow, RestrictionDepth::FirstMove)
            )
            .await,
            Err(AppError::AnalysisTools(AnalysisToolsError::GameActive))
        ));
    }
    within(h.app.shutdown()).await.unwrap();
}
