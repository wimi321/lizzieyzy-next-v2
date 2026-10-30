#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{ColorView, MoveRank, MoveRankEvaluationMode, StoredAnalysis, actor::tests::actor};
use serde_json::json;

fn loaded(sgf: &[u8]) -> Actor {
    let (mut actor, _) = actor();
    let collection = sgf::parse(sgf).unwrap();
    actor.document = document::GameDocument::from_sgf(&collection.games[0]).unwrap();
    actor.revision = 10;
    actor.publish_document().unwrap();
    actor
}

fn retain(actor: &mut Actor, node: u32, best_winrate: f64, best_score: f64) {
    let id = NodeId(node);
    let position = actor.document.position_at(id).unwrap();
    let response = serde_json::from_value(json!({
        "id": "problem-move-unit", "turnNumber": position.analysis_moves.len(),
        "isDuringSearch": false,
        "rootInfo": { "visits": 500 + node, "winrate": 0.5, "scoreLead": 0.0,
            "utility": 0.0, "currentPlayer": if position.board.to_play() == go_core::Color::Black { "B" } else { "W" } },
        "moveInfos": [{ "move": "H9", "visits": 400, "winrate": best_winrate,
            "scoreLead": best_score, "prior": 0.2, "order": 0, "pv": ["H9"] }]
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

fn all() -> ProblemMovesQuery {
    ProblemMovesQuery {
        minimum_rank: MoveRank::Best,
        ..Default::default()
    }
}

fn entry_ids(view: &ProblemMovesView) -> Vec<u32> {
    view.entries.iter().map(|entry| entry.node_id).collect()
}

#[test]
fn java_sort_uses_winrate_loss_then_earliest_move_independent_of_grade() {
    let mut actor = loaded(b"(;SZ[9];B[aa];W[bb];B[cc];W[dd])");
    actor.settings.move_rank.mode = MoveRankEvaluationMode::Score;
    for (id, winrate, score) in [
        (0, 0.75, 0.5),
        (1, 0.375, -12.0),
        (2, 0.75, 1.5),
        (3, 0.125, -3.0),
        (4, 0.5, 0.0),
    ] {
        retain(&mut actor, id, winrate, score);
    }
    let view = actor.problem_moves(all()).unwrap();
    assert_eq!(entry_ids(&view), [4, 1, 3, 2]);
    assert_eq!(
        view.entries
            .iter()
            .map(|entry| entry.loss.rank)
            .collect::<Vec<_>>(),
        [
            MoveRank::Inaccuracy,
            MoveRank::Good,
            MoveRank::Normal,
            MoveRank::Blunder
        ]
    );
    assert_eq!(view.entries[0].loss.visits, 504);
    assert_eq!(view.entries[0].loss.before_visits, 503);
    assert_eq!(view.entries[0].coordinate, "D6");

    for (sort, expected) in [
        (ProblemMoveSort::MoveAsc, vec![1, 2, 3, 4]),
        (ProblemMoveSort::MoveDesc, vec![4, 3, 2, 1]),
    ] {
        assert_eq!(
            entry_ids(
                &actor
                    .problem_moves(ProblemMovesQuery { sort, ..all() })
                    .unwrap()
            ),
            expected
        );
    }
    let filtered = actor
        .problem_moves(ProblemMovesQuery {
            minimum_rank: MoveRank::Normal,
            color: Some(ColorView::Black),
            ..all()
        })
        .unwrap();
    assert_eq!(entry_ids(&filtered), [3]);
    assert_eq!(filtered.error_bar, view.error_bar);
    actor
        .review
        .results
        .get_mut(&NodeId(0))
        .unwrap()
        .response
        .move_infos[0]
        .coordinate = "A9".into();
    let best = actor.problem_moves(all()).unwrap();
    assert_eq!(entry_ids(&best), [4, 3, 2]);
    assert_eq!(best.error_bar[1].loss.as_ref().unwrap().winrate_loss, 0.0);
    assert!(best.error_bar[1].loss.as_ref().unwrap().is_best);
}

#[test]
fn displayed_variation_keeps_empty_nodes_passes_and_missing_analysis_aligned() {
    let mut actor = loaded(b"(;SZ[9];B[aa];C[empty](;W[];B[cc])(;W[dd];B[ee]))");
    for node in [0, 1, 3, 4, 5, 6] {
        retain(&mut actor, node, 0.75, 5.0);
    }
    let original = actor.document.snapshot();
    let results = actor.review.results.clone();
    let view = actor.problem_moves(all()).unwrap();
    assert_eq!(
        view.error_bar.iter().map(|p| p.node_id).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );
    assert!(view.error_bar[0].loss.is_none());
    assert!(view.error_bar[2].mv.is_none());
    assert!(view.error_bar[2].loss.is_none());
    assert!(view.error_bar[3].loss.is_none(), "missing parent result");
    assert_eq!(
        view.error_bar[3].mv,
        Some(crate::MoveView::Pass {
            color: ColorView::White
        })
    );
    assert!(view.error_bar[4].loss.is_some());
    assert_eq!(actor.document.snapshot(), original);
    assert_eq!(actor.review.results, results);
    assert_eq!(actor.revision, 10);
    assert_eq!(
        actor.document.undo(),
        Err(document::DocError::NothingToUndo)
    );

    retain(&mut actor, 2, 0.25, -5.0);
    let with_pass = actor.problem_moves(all()).unwrap();
    assert_eq!(
        with_pass
            .entries
            .iter()
            .find(|entry| entry.node_id == 3)
            .unwrap()
            .coordinate,
        "pass"
    );
    actor
        .document
        .apply(document::Command::GoTo(NodeId(5)))
        .unwrap();
    actor.publish_document().unwrap();
    let branch = actor.problem_moves(all()).unwrap();
    assert_eq!(
        branch
            .error_bar
            .iter()
            .map(|p| p.node_id)
            .collect::<Vec<_>>(),
        [0, 1, 2, 5, 6]
    );
    assert_eq!(
        branch
            .error_bar
            .iter()
            .map(|p| p.move_number)
            .collect::<Vec<_>>(),
        [0, 1, 1, 2, 3]
    );
    assert_eq!(branch.current_index, 3);
    assert!(
        branch
            .entries
            .iter()
            .all(|entry| ![3, 4].contains(&entry.node_id))
    );
}

#[test]
fn incomparable_root_moves_setup_and_turn_resets_have_empty_loss() {
    for sgf in [
        b"(;SZ[9]B[aa];W[bb])".as_slice(),
        b"(;SZ[9];AB[cc]B[aa];W[bb])".as_slice(),
        b"(;SZ[9];PL[W]W[aa];B[bb])".as_slice(),
        b"(;SZ[9];W[aa];B[bb])".as_slice(),
    ] {
        let mut actor = loaded(sgf);
        let ids: Vec<_> = actor.tree.nodes.iter().map(|node| node.id).collect();
        for node in ids {
            retain(&mut actor, node, 0.75, 5.0);
        }
        let view = actor.problem_moves(all()).unwrap();
        assert!(view.error_bar[0].loss.is_none());
        if view.error_bar.len() == 3 {
            assert!(view.error_bar[1].loss.is_none());
        }
        assert!(view.error_bar.last().unwrap().loss.is_some());
    }
}

#[test]
fn stale_keys_illegal_nodes_and_deleted_variations_never_leak_losses() {
    let mut actor = loaded(b"(;SZ[9];B[aa];W[bb];B[cc])");
    for node in 0..=3 {
        retain(&mut actor, node, 0.75, 5.0);
    }
    let key = actor.review.results[&NodeId(0)].key;
    let valid = actor.review.results[&NodeId(2)].key;
    actor.review.results.get_mut(&NodeId(2)).unwrap().key = key;
    let stale = actor.problem_moves(all()).unwrap();
    assert!(stale.error_bar[1].loss.is_some());
    assert!(stale.error_bar[2..].iter().all(|p| p.loss.is_none()));
    actor.review.results.get_mut(&NodeId(2)).unwrap().key = valid;
    actor
        .document
        .apply(document::Command::DeleteNode(NodeId(2)))
        .unwrap();
    actor.publish_document().unwrap();
    assert_eq!(actor.problem_moves(all()).unwrap().error_bar.len(), 2);
    actor.document.undo().unwrap();
    actor.publish_document().unwrap();
    assert!(
        actor.problem_moves(all()).unwrap().error_bar[2..]
            .iter()
            .all(|p| p.loss.is_some())
    );

    let mut invalid = loaded(b"(;SZ[9];B[aa];W[aa];B[cc])");
    retain(&mut invalid, 0, 0.75, 5.0);
    retain(&mut invalid, 1, 0.75, 5.0);
    // Even a retained result under a reused-looking node id cannot bypass replay.
    let mut payload = invalid.review.results[&NodeId(1)].clone();
    payload.node_id = NodeId(2);
    invalid.review.results.insert(NodeId(2), payload);
    let view = invalid.problem_moves(all()).unwrap();
    assert!(view.error_bar[2..].iter().all(|p| p.loss.is_none()));
    assert_eq!(view.error_bar.len(), 4);
}
