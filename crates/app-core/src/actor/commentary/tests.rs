#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::actor::tests::actor;

#[tokio::test]
async fn mainline_range_uses_preceding_positions_while_next_move_can_use_a_branch() {
    let (mut app, _events) = actor();
    app.open(
        include_bytes!("../../../../../fixtures/lz/java-analysis.sgf").to_vec(),
        None,
    )
    .await
    .unwrap();
    for (mode, target, position) in [
        (CommentaryMode::NextMove, 0, 0),
        (CommentaryMode::Range { from: 2, to: 2 }, 2, 1),
        (CommentaryMode::WholeGame, 0, 0),
    ] {
        let (evidence, node) = app
            .commentary_evidence(&mode, ::commentary::Language::ZhCn)
            .unwrap();
        assert_eq!(node.0, target);
        assert_eq!(evidence.positions[0].node_id, position);
        assert!(!evidence.text.contains("manual comment"));
    }
    for (from, to) in [(0, 1), (2, 1), (1, 3)] {
        assert!(matches!(
            app.commentary_evidence(
                &CommentaryMode::Range { from, to },
                ::commentary::Language::En
            ),
            Err(AppError::Commentary(CommentaryError::InvalidRange))
        ));
    }
}

#[tokio::test]
async fn illegal_recorded_continuation_is_reported_by_local_replay() {
    let (mut app, _events) = actor();
    let sgf =
        String::from_utf8(include_bytes!("../../../../../fixtures/lz/java-analysis.sgf").to_vec())
            .unwrap()
            .replace(";W[bb]", ";W[aa]");
    app.open(sgf.into_bytes(), None).await.unwrap();
    app.document
        .apply(document::Command::GoTo(NodeId(1)))
        .unwrap();
    let (evidence, _) = app
        .commentary_evidence(&CommentaryMode::NextMove, ::commentary::Language::En)
        .unwrap();
    assert!(
        evidence.text.contains("illegal") || evidence.text.contains("Illegal"),
        "{}",
        evidence.text
    );
}
