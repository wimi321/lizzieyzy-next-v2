#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{StoredAnalysis, actor::tests, test_support::within};

#[tokio::test]
async fn ownership_premarks_whole_groups_in_black_perspective_even_on_white_turn() {
    let (mut actor, _events) = tests::actor();
    let mut snapshot = tests::snapshot(&actor, 100, 0.5);
    within(actor.open(
        b"(;SZ[5]KM[0]RU[japanese]PL[W]AB[aa][ba][da]AW[ad][bd][dd])".to_vec(),
        None,
    ))
    .await
    .unwrap();
    snapshot.revision = actor.revision;
    snapshot.key = actor
        .document
        .position_at(actor.document.current())
        .unwrap()
        .analysis_key();
    let mut ownership = vec![0.0; 25];
    ownership[0] = -1.0;
    ownership[1] = -0.7; // mean -0.85: both black stones are marked.
    ownership[3] = -0.79; // below threshold: leave alive.
    ownership[15] = 1.0;
    ownership[16] = 0.7; // mean +0.85: both white stones are marked.
    ownership[18] = -1.0; // White's own territory: leave alive.
    snapshot.response.ownership = Some(ownership);
    actor.review.results.insert(
        snapshot.node_id,
        StoredAnalysis {
            node_id: snapshot.node_id,
            key: snapshot.key,
            response: snapshot.response,
            saved: None,
        },
    );
    let view = within(actor.start_scoring()).await.unwrap();
    assert_eq!(view.rule, ScoringRuleView::Territory);
    assert_eq!(
        view.dead_stones,
        vec![
            PointView { x: 0, y: 0 },
            PointView { x: 1, y: 0 },
            PointView { x: 0, y: 3 },
            PointView { x: 1, y: 3 }
        ]
    );
    assert_eq!(view.black.dead_stones, 2);
    assert_eq!(view.white.dead_stones, 2);
}

#[test]
fn incomplete_or_invalid_ownership_is_never_used_for_dead_group_guessing() {
    let tree = sgf::parse(b"(;SZ[3]AB[aa][ba])").unwrap().games.remove(0);
    let doc = document::GameDocument::from_sgf(&tree).unwrap();
    let position = doc.position_at(doc.root()).unwrap();
    assert!(initial_dead(&position, None).is_empty());
    assert!(initial_dead(&position, Some(&[-1.0; 8])).is_empty());
    for value in [f64::NAN, f64::INFINITY, -1.1] {
        let mut ownership = [-1.0; 9];
        ownership[1] = value;
        assert!(initial_dead(&position, Some(&ownership)).is_empty());
    }
}
