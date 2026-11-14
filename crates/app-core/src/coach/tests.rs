use super::*;
use document::{GameDocument, GamePosition};
use katago_protocol::AnalysisResponse;
use serde_json::json;

pub(super) fn position(sgf: &str) -> GamePosition {
    let tree = sgf::parse(sgf.as_bytes()).unwrap().games.remove(0);
    let mut document = GameDocument::from_sgf(&tree).unwrap();
    document.apply(document::Command::Step(1000)).unwrap();
    document.position_at(document.current()).unwrap()
}

pub(super) fn response() -> AnalysisResponse {
    serde_json::from_value(json!({
        "id":"coach", "turnNumber":0, "isDuringSearch":false,
        "rootInfo":{"visits":400,"winrate":0.5,"scoreLead":0.0,"utility":0.0,
                    "currentPlayer":"B", "humanScoreStdev":12.0},
        "moveInfos":[
            {"move":"A9","visits":200,"winrate":0.5,"scoreLead":0.0,
             "prior":0.5,"order":0,"pv":["A9"]},
            {"move":"B9","visits":100,"winrate":0.2,"scoreLead":-12.0,
             "prior":0.3,"order":1,"pv":["B9"]},
            {"move":"C9","visits":1,"winrate":0.1,"scoreLead":-20.0,
             "prior":0.2,"order":2,"pv":["C9"]}
        ], "humanPolicy":null
    }))
    .unwrap()
}

#[test]
fn twenty_nine_ranks_defaults_and_one_time_start_setting() {
    let mut ranks = Vec::new();
    for rank in 1..=20 {
        ranks.push(CoachRank::Kyu(rank));
    }
    for rank in 1..=9 {
        ranks.push(CoachRank::Dan(rank));
    }
    assert_eq!(ranks.len(), 29);
    for rank in ranks {
        rank.validate().unwrap();
        assert!(rank.profile().starts_with("rank_"));
    }
    for rank in [
        CoachRank::Kyu(0),
        CoachRank::Kyu(21),
        CoachRank::Dan(0),
        CoachRank::Dan(10),
    ] {
        assert!(rank.validate().is_err());
    }
    let setup: CoachSetup = serde_json::from_value(json!({})).unwrap();
    assert_eq!(setup, CoachSetup::default());
    assert_eq!(setup.rank, CoachRank::Dan(3));
    assert_eq!(setup.mode, CoachMode::LiveFeedback);
    assert_eq!(setup.color, CoachColor::Random);
    assert_eq!(setup.pace, CoachPace::Natural);
    assert_eq!(setup.komi(), 7.5);
    let setup = CoachSetup {
        from_current_position: true,
        handicap: 1,
        ..setup
    };
    setup.validate().unwrap();
    assert!(!setup.persisted().from_current_position);
    assert_eq!(setup.persisted().handicap, 1);
    assert_eq!(setup.komi(), 0.5);
    for width in [9, 13, 19] {
        CoachSetup {
            width,
            height: width,
            handicap: 9,
            ..Default::default()
        }
        .validate()
        .unwrap();
    }
    assert!(
        CoachSetup {
            width: 10,
            height: 10,
            handicap: 2,
            ..Default::default()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn summary_retains_counts_reason_and_three_worst_moves() {
    let mut response = response();
    let position = position("(;SZ[9])");
    let mut ratings = Vec::new();
    for (index, loss) in [2.0, 40.0, 6.0, 20.0, 10.0].into_iter().enumerate() {
        response.move_infos[1].score_lead = -loss;
        ratings.push(
            rate_move(
                &response,
                None,
                &position,
                go_core::Move::Play(go_core::Color::Black, go_core::Point { x: 1, y: 0 }),
                index as u32 + 10,
                index as u32 + 1,
            )
            .unwrap(),
        );
    }
    let summary =
        CoachSummary::from_ratings(None, Some(GameEndReason::Stopped), 55, 3, 2, &ratings);
    assert_eq!(summary.total_moves, 55);
    assert_eq!(summary.end_reason, Some(GameEndReason::Stopped));
    assert_eq!(
        (summary.hints, summary.take_backs, summary.blunders),
        (3, 2, 3)
    );
    assert_eq!(
        summary
            .worst_moves
            .iter()
            .map(|m| m.node_id)
            .collect::<Vec<_>>(),
        [11, 13, 14]
    );
    assert_eq!(summary.worst_moves[0].points_lost, 40.0);
}
