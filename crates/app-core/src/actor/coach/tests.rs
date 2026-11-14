#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{
    CoachColor, CoachDecision, CoachHint, CoachPace, CoachRank, CoachVerdict, GameEndReason,
};
use serde_json::json;

mod failures;
mod startup;
mod support;
use support::*;

#[tokio::test]
async fn first_start_loads_main_engine_hints_have_two_steps_and_review_restores_plain_analysis() {
    let mut h = harness(feedback_script(), false).await;
    start(&mut h.actor, setup(CoachMode::LiveFeedback))
        .await
        .unwrap();
    assert_eq!(h.actor.coach_hint().unwrap(), CoachHint::Reading);
    assert!(
        h.actor
            .settings
            .engine
            .as_ref()
            .unwrap()
            .human_model
            .is_some()
    );
    cached(&mut h.actor).await;
    assert_eq!(
        h.actor.coach_hint().unwrap(),
        CoachHint::Region { row: 0, column: 0 }
    );
    let CoachHint::Exact(best) = h.actor.coach_hint().unwrap() else {
        panic!("second hint must be exact");
    };
    assert_eq!(
        best.mv,
        Move::Play(Color::Black, go_core::Point { x: 0, y: 0 }).into()
    );
    assert_eq!(best.human_share, Some(0.05));
    assert!(matches!(h.actor.coach_hint().unwrap(), CoachHint::Exact(_)));
    assert_eq!(h.actor.coach_view().unwrap().hints, 1);
    assert_eq!(
        crate::Settings::load(h.directory.path())
            .await
            .unwrap()
            .coach_setup,
        setup(CoachMode::LiveFeedback)
    );
    let before = requests_before(&h.actor, h.directory.path()).await;
    let human: Vec<_> = before
        .iter()
        .filter(|r| r["overrideSettings"]["humanSLProfile"].is_string())
        .collect();
    assert!(!human.is_empty());
    for request in human {
        assert_eq!(request["includePolicy"], true);
        assert_eq!(request["overrideSettings"]["ignorePreRootHistory"], false);
        assert_eq!(request["overrideSettings"]["rootNumSymmetriesToSample"], 1);
        assert_eq!(
            request["overrideSettings"]["humanSLRootExploreProbWeightless"],
            0.5
        );
    }
    let launches: Vec<serde_json::Value> =
        std::fs::read_to_string(h.directory.path().join("launch.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    assert_eq!(
        launches.len(),
        2,
        "first start restarts the same main owner once"
    );
    assert!(launches[1].to_string().contains("-human-model"));
    h.actor.coach_review().await.unwrap();
    let prefix = requests_before(&h.actor, h.directory.path()).await;
    let plain = prefix
        .iter()
        .find(|r| r["maxVisits"] == 20)
        .expect("normal live query resumed");
    assert!(plain["overrideSettings"].get("humanSLProfile").is_none());
    assert_eq!(
        h.actor.game_view().unwrap().end_reason,
        Some(GameEndReason::Stopped)
    );
    let summary = h.actor.coach_view().unwrap().summary.unwrap();
    assert_eq!(
        (summary.total_moves, summary.hints, summary.take_backs),
        (0, 1, 0)
    );
    assert_eq!(summary.end_reason, Some(GameEndReason::Stopped));
    assert!(h.actor.analysis_state().pondering);
    assert!(!h.actor.coach_hides_analysis());
    close(h).await;
}

#[tokio::test]
async fn training_modes_submit_only_their_permitted_live_or_feedback_queries() {
    for mode in [
        CoachMode::LiveFeedback,
        CoachMode::ReviewAfter,
        CoachMode::LiveAnalysis,
    ] {
        let mut h = harness(feedback_script(), true).await;
        start(&mut h.actor, setup(mode)).await.unwrap();
        let prefix = requests_before(&h.actor, h.directory.path()).await;
        let live = prefix.iter().filter(|r| r["maxVisits"] == 20).count();
        let review = prefix.iter().filter(|r| r["maxVisits"] == 400).count();
        assert_eq!(
            live,
            usize::from(mode == CoachMode::LiveAnalysis),
            "{mode:?}"
        );
        assert_eq!(
            review,
            usize::from(mode == CoachMode::LiveFeedback),
            "{mode:?}"
        );
        assert_eq!(
            h.actor.coach_hides_analysis(),
            mode != CoachMode::LiveAnalysis
        );
        if mode != CoachMode::LiveFeedback {
            assert!(matches!(
                h.actor.coach_hint(),
                Err(AppError::Coach(CoachError::HintUnavailable))
            ));
        }
        close(h).await;
    }
}

#[tokio::test]
async fn blunder_holds_prepared_reply_show_best_preserves_wait_continue_plays_retry_counts() {
    for retry in [false, true] {
        let mut h = harness(feedback_script(), true).await;
        start(&mut h.actor, setup(CoachMode::LiveFeedback))
            .await
            .unwrap();
        cached(&mut h.actor).await;
        h.actor
            .apply(super::super::Input::Edit(document::Command::Play(
                Move::Play(Color::Black, go_core::Point { x: 2, y: 0 }),
            )))
            .await
            .unwrap();
        assert!(h.actor.coach_view().unwrap().awaiting_decision);
        let rating = within(async {
            loop {
                if let AppEvent::CoachMoveRated(rating) = h.output.recv().await.unwrap() {
                    break rating;
                }
            }
        })
        .await;
        assert_eq!(rating.verdict, CoachVerdict::Blunder);
        assert_eq!(rating.move_number, 1);
        assert_eq!(rating.points_lost, 25.0);
        selected(&mut h.actor).await;
        let decision = within(h.actor.coach_decision(CoachDecision::ShowBest))
            .await
            .unwrap();
        assert!(matches!(decision, CoachHint::Exact(_)));
        assert!(h.actor.coach_view().unwrap().awaiting_decision);
        tokio::time::advance(Duration::from_secs(14)).await;
        h.actor.maybe_coach_move().await.unwrap();
        assert_eq!(
            h.actor.game_view().unwrap().move_number,
            1,
            "blunder waits beyond pace"
        );
        if retry {
            within(h.actor.coach_decision(CoachDecision::Retry))
                .await
                .unwrap();
            assert_eq!(h.actor.game_view().unwrap().move_number, 0);
            let view = h.actor.coach_view().unwrap();
            assert_eq!(view.take_backs, 1);
            assert!(!view.awaiting_decision);
            assert!(view.last_rating.is_none());
            assert!(h.actor.document.node(NodeId(rating.node_id)).is_err());
            cached(&mut h.actor).await;
        } else {
            within(h.actor.coach_decision(CoachDecision::Continue))
                .await
                .unwrap();
            assert_eq!(h.actor.game_view().unwrap().move_number, 2);
            assert!(!h.actor.coach_view().unwrap().awaiting_decision);
        }
        h.actor
            .game_command(crate::game_api::Request::Stop)
            .await
            .unwrap();
        let summary = h.actor.coach_view().unwrap().summary.unwrap();
        assert_eq!(summary.take_backs, u32::from(retry));
        assert_eq!(summary.blunders, u32::from(!retry));
        assert_eq!(summary.end_reason, Some(GameEndReason::Stopped));
        close(h).await;
    }
}

#[tokio::test(start_paused = true)]
async fn prepared_ai_move_cannot_appear_before_its_tokio_pace_deadline() {
    // The harness pauses after external process setup; stdio waits keep time fixed.
    tokio::time::resume();
    let mut h = harness(feedback_script(), true).await;
    let mut options = setup(CoachMode::ReviewAfter);
    options.color = CoachColor::White;
    options.pace = CoachPace::Slow;
    start(&mut h.actor, options).await.unwrap();
    selected(&mut h.actor).await;
    let ai = h.actor.coach.session.as_ref().unwrap().ai.as_ref().unwrap();
    let at = ai.ready_at.unwrap();
    let delay = at - ai.started;
    assert!((Duration::from_millis(800)..=Duration::from_secs(14)).contains(&delay));
    let remaining = at.saturating_duration_since(Instant::now());
    assert!(remaining > Duration::from_millis(1));
    tokio::time::advance(remaining - Duration::from_millis(1)).await;
    h.actor.maybe_coach_move().await.unwrap();
    assert_eq!(h.actor.game_view().unwrap().move_number, 0);
    tokio::time::advance(Duration::from_millis(1)).await;
    h.actor.maybe_coach_move().await.unwrap();
    assert_eq!(h.actor.game_view().unwrap().move_number, 1);
    close(h).await;
}

#[tokio::test]
async fn high_dan_search_uses_allow_moves_and_bounded_deepening() {
    for rank in [CoachRank::Dan(6), CoachRank::Dan(9)] {
        let profile = rank.profile();
        let script = json!({"profiles":{profile:{"default":{
            "humanPolicy":policy(&[(0,0.6),(1,0.4)],81),"policy":policy(&[(2,1.0)],81),
            "candidates":[{"move":"A9","visitShare":0.6,"scoreLead":0},
                          {"move":"B9","visitShare":0.3,"scoreLead":0},
                          {"move":"C9","visitShare":0.1,"scoreLead":0}]
        }}}});
        let mut h = harness(script, true).await;
        let mut options = setup(CoachMode::ReviewAfter);
        options.color = CoachColor::White;
        options.rank = rank;
        start(&mut h.actor, options).await.unwrap();
        selected(&mut h.actor).await;
        let prefix = requests_before(&h.actor, h.directory.path()).await;
        let searches: Vec<_> = prefix
            .iter()
            .filter(|r| r.get("allowMoves").is_some())
            .collect();
        assert!(!searches.is_empty());
        assert_eq!(searches[0]["maxVisits"], 64);
        assert!(searches.len() <= 5);
        assert!(
            searches
                .iter()
                .all(|r| r["maxVisits"].as_u64().unwrap() <= 4096)
        );
        if rank.elite() {
            assert!(searches.len() > 1);
        } else {
            assert_eq!(searches.len(), 1);
        }
        let allowed = searches[0]["allowMoves"][0]["moves"].as_array().unwrap();
        for name in ["A9", "B9", "C9"] {
            assert!(allowed.iter().any(|v| v == name));
        }
        for request in searches {
            assert_eq!(
                request["overrideSettings"]["humanSLRootExploreProbWeightless"],
                if rank.elite() { 0.8 } else { 0.5 }
            );
        }
        close(h).await;
    }
}

#[tokio::test]
async fn handicap_is_placed_on_nine_thirteen_nineteen_and_current_position_is_not_persisted() {
    for width in [9, 13, 19] {
        let mut h = harness(json!({}), true).await;
        let mut options = setup(CoachMode::ReviewAfter);
        options.width = width;
        options.height = width;
        options.handicap = 4;
        options.color = CoachColor::White;
        start(&mut h.actor, options).await.unwrap();
        let position = h
            .actor
            .document
            .position_at(h.actor.document.root())
            .unwrap();
        assert_eq!(position.board.stones().iter().flatten().count(), 4);
        assert_eq!(position.komi, 0.5);
        assert_eq!(position.board.to_play(), Color::White);
        assert_eq!(
            h.actor.coach_view().unwrap().human_color,
            crate::ColorView::Black
        );
        close(h).await;
    }
    let mut h = harness(json!({}), true).await;
    h.actor
        .apply(super::super::Input::OpenBytes(b"(;SZ[9];B[dd])".to_vec()))
        .await
        .unwrap();
    h.actor
        .apply(super::super::Input::Edit(document::Command::Step(1)))
        .await
        .unwrap();
    let options = CoachSetup {
        from_current_position: true,
        color: CoachColor::White,
        ..setup(CoachMode::ReviewAfter)
    };
    start(&mut h.actor, options).await.unwrap();
    assert_eq!(h.actor.game_view().unwrap().move_number, 1);
    assert!(
        !crate::Settings::load(h.directory.path())
            .await
            .unwrap()
            .coach_setup
            .from_current_position
    );
    close(h).await;
}

#[tokio::test]
async fn all_pass_policy_still_uses_plain_pass_search_and_finishes_two_passes() {
    let script = json!({"default":{"candidates":[{"move":"pass","visitShare":1.0}],"scoreLead":2.5},
        "profiles":{"rank_3d":{"default":{"humanPolicy":policy(&[(81,1.0)],81),"candidates":[{"move":"pass","visitShare":1.0}]}}}});
    let mut h = harness(script, true).await;
    start(&mut h.actor, setup(CoachMode::ReviewAfter))
        .await
        .unwrap();
    h.actor
        .apply(super::super::Input::Edit(document::Command::Play(
            Move::Pass(Color::Black),
        )))
        .await
        .unwrap();
    deliver(&mut h.actor).await;
    let request = query(h.directory.path(), |r| r["maxVisits"] == 40).await;
    assert!(request["overrideSettings"].get("humanSLProfile").is_none());
    within(async {
        while h.actor.game_view().unwrap().phase != GamePhase::Finished {
            let event = super::super::game::receive(&mut h.actor.game).await;
            h.actor.game_signal(event).await;
        }
    })
    .await;
    assert_eq!(
        h.actor.game_view().unwrap().end_reason,
        Some(GameEndReason::Scored)
    );
    assert_eq!(
        h.actor.coach_view().unwrap().summary.unwrap().total_moves,
        2
    );
    close(h).await;
}
