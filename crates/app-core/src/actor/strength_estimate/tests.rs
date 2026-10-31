#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{StoredAnalysis, actor::tests::actor};
use serde_json::json;

fn loaded() -> Actor {
    let (mut actor, _) = actor();
    let games = sgf::parse(b"(;SZ[9];B[aa];C[empty](;W[bb];B[])(;W[cc];B[dd]))").unwrap();
    actor.document = document::GameDocument::from_sgf(&games.games[0]).unwrap();
    actor.revision = 10;
    actor.publish_document().unwrap();
    for node in 0..=6 {
        let id = NodeId(node);
        let position = actor.document.position_at(id).unwrap();
        let white = position.board.to_play() == go_core::Color::White;
        let (top_winrate, top_score, actual_winrate, actual_score) = if white {
            (0.2, -5.0, 0.3, -3.0)
        } else {
            (0.8, 5.0, 0.7, 3.0)
        };
        let actual = match node {
            0 => "A9",
            2 => "B8",
            3 => "pass",
            5 => "D6",
            _ => "E5",
        };
        let response = serde_json::from_value(json!({
            "id": "strength", "turnNumber": position.analysis_moves.len(), "isDuringSearch": false,
            "rootInfo": {"visits": 500, "winrate": 0.5, "scoreLead": 0.0, "utility": 0.0, "currentPlayer": if white {"W"} else {"B"}},
            "moveInfos": [
                {"move": actual, "visits": 100, "winrate": actual_winrate, "scoreLead": actual_score, "prior": 0.4, "order": 1, "pv": [actual]},
                {"move": "H9", "visits": 400, "winrate": top_winrate, "scoreLead": top_score, "prior": 0.6, "order": 0, "pv": ["H9"]}
            ]
        })).unwrap();
        actor.review.results.insert(
            id,
            StoredAnalysis {
                node_id: id,
                key: position.analysis_key(),
                response,
            },
        );
    }
    actor
}

#[test]
fn candidate_losses_use_player_perspective_and_current_line_without_side_effects() {
    let mut actor = loaded();
    let document = actor.document.snapshot();
    let retained = actor.review.results.clone();
    let estimate = actor.strength_estimate(None).unwrap();
    assert_eq!((estimate.total_moves, estimate.analyzed_moves), (3, 3));
    for side in [&estimate.report.black, &estimate.report.white] {
        assert_eq!(side.average_score_loss, Some(2.0));
        assert!((side.average_winrate_loss - 10.0).abs() < 1e-6);
    }
    assert_eq!(estimate.report.black.move_rank_counts, [0, 0, 2, 0, 0, 0]);
    assert_eq!(estimate.report.white.move_rank_counts, [0, 0, 1, 0, 0, 0]);
    assert_eq!(estimate.phases.phases[0].black, estimate.report.black);
    assert_eq!(estimate.phases.phases[0].white, estimate.report.white);
    assert!(!estimate.phases.phases[0].black_ranked);
    assert!(!estimate.phases.phases[0].white_ranked);
    assert!(estimate.phases.phases[0].played);
    assert!(!estimate.phases.phases[1].played);
    assert!(!estimate.phases.phases[2].played);
    assert_eq!(actor.document.snapshot(), document);
    assert_eq!(actor.review.results, retained);
    assert_eq!(actor.revision, 10);
    actor
        .document
        .apply(document::Command::GoTo(NodeId(5)))
        .unwrap();
    actor.publish_document().unwrap();
    let branch = actor.strength_estimate(None).unwrap();
    assert_eq!((branch.total_moves, branch.analyzed_moves), (3, 3));
    assert_eq!(branch.report.white.samples[0].coordinate, "C7");
    // Java strength falls back to roots (both zero here), not P1-24's best
    // candidate (-5 for White) versus the after-position root.
    assert_eq!(branch.report.white.samples[0].score_loss, Some(0.0));
    assert_eq!(branch.report.white.samples[0].winrate_loss, 0.0);
    assert_eq!(branch.report.white.samples[0].ai_rank, 999);
    assert_eq!(branch.report.black.samples[1].coordinate, "D6");
    assert_eq!(branch.phases.phases[0].black, branch.report.black);
    assert_eq!(branch.phases.phases[0].white, branch.report.white);
    assert_ne!(branch.phases, estimate.phases);
}

#[test]
fn missing_or_stale_analysis_changes_coverage_and_cannot_be_cached_as_a_report() {
    let mut actor = loaded();
    let original = actor.strength_estimate(None).unwrap();
    assert_eq!(original.analyzed_moves, 3);
    let saved = actor.review.results.remove(&NodeId(2)).unwrap();
    let partial = actor.strength_estimate(None).unwrap();
    assert_eq!(partial.analyzed_moves, 2);
    assert_eq!(partial.phases.phases[0].black.sample_count, 2);
    assert_eq!(partial.phases.phases[0].white.sample_count, 0);
    actor.review.results.insert(NodeId(2), saved);
    assert_eq!(actor.strength_estimate(None).unwrap(), original);
    actor.review.results.get_mut(&NodeId(3)).unwrap().key = document::AnalysisKey([0; 32]);
    let stale = actor.strength_estimate(None).unwrap();
    assert_eq!(stale.analyzed_moves, 1);
    assert_eq!(stale.phases.phases[0].black.sample_count, 1);
    assert_eq!(stale.phases.phases[0].white.sample_count, 0);
    actor.review.results.clear();
    let empty = actor.strength_estimate(None).unwrap();
    assert_eq!((empty.total_moves, empty.analyzed_moves), (3, 0));
    assert!(!empty.report.has_enough_data);
    assert!(empty.phases.phases[0].played);
    for phase in empty.phases.phases {
        assert_eq!((phase.black.sample_count, phase.white.sample_count), (0, 0));
        assert!(!phase.black_ranked && !phase.white_ranked);
    }
}

#[test]
fn java_root_fallback_preserves_black_and_white_loss_direction() {
    let mut actor = loaded();
    for node in [0, 2] {
        actor
            .review
            .results
            .get_mut(&NodeId(node))
            .unwrap()
            .response
            .move_infos[0]
            .coordinate = "F5".into();
    }
    for (node, winrate, score) in [
        (0, 0.8, 8.0),
        (1, 0.6, 3.0),
        (2, 0.25, -8.0),
        (3, 0.5, -3.0),
    ] {
        let root = &mut actor
            .review
            .results
            .get_mut(&NodeId(node))
            .unwrap()
            .response
            .root_info;
        root.winrate = winrate;
        root.score_lead = score;
    }
    let report = actor.strength_estimate(None).unwrap().report;
    let black = &report.black.samples[0];
    let white = &report.white.samples[0];
    assert!((black.winrate_loss - 20.0).abs() < 1e-6);
    assert_eq!(black.score_loss, Some(5.0));
    assert_eq!(white.winrate_loss, 25.0);
    assert_eq!(white.score_loss, Some(5.0));
    assert_eq!((black.ai_rank, white.ai_rank), (999, 999));
}

#[test]
fn malformed_last_move_still_reaches_the_next_phase_on_the_displayed_line() {
    let (mut actor, _) = actor();
    let text = format!("(;SZ[9]{};B[invalid])", ";B[];W[]".repeat(30));
    let games = sgf::parse(text.as_bytes()).unwrap();
    actor.document = document::GameDocument::from_sgf(&games.games[0]).unwrap();
    actor.publish_document().unwrap();
    let estimate = actor.strength_estimate(None).unwrap();
    // Keep the pre-existing valid-move coverage count unchanged.
    assert_eq!((estimate.total_moves, estimate.analyzed_moves), (60, 0));
    let middle = &estimate.phases.phases[1];
    assert_eq!((middle.first, middle.last), (61, 61));
    assert!(middle.played);
    assert!(!middle.black_ranked && !middle.white_ranked);
    assert!(!estimate.phases.phases[2].played);
}
