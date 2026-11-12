#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{
    AppEvent, AppSubscription, DocumentView, WholeGameProgress, WholeGameSource, WholeGameStatus,
    actor::{Input, Signal, ViewFocus, tests::actor, tests::snapshot},
};
use analysis::{AnalysisSnapshot, review::ReviewResult};
use document::Command;
use go_core::{Color, Move, Point};
use std::{future::Future, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("graph validation test exceeded safety deadline")
}

async fn document(output: &mut AppSubscription) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = output.recv().await.unwrap() {
                return view;
            }
        }
    })
    .await
}

fn incoming(actor: &mut Actor, node: NodeId, visits: u64) -> AnalysisSnapshot {
    let position = actor.document.position_at(node).unwrap();
    actor.focus = Some(ViewFocus {
        revision: actor.revision,
        node_id: node.0,
        key: position.analysis_key(),
        size: position.board.size(),
        to_play: position.board.to_play(),
    });
    let mut value = snapshot(actor, visits, 0.6);
    value.node_id = node;
    value
}

fn validation_work(actor: &Actor) -> (usize, usize) {
    (
        actor.graph_analysis.recomputations,
        actor.graph_analysis.replays,
    )
}

#[tokio::test]
async fn navigation_and_analysis_publish_without_revalidating_graph_but_content_edits_do() {
    let (mut actor, mut output) = actor();
    within(actor.apply(Input::OpenBytes(
        b"(;SZ[9]KM[7.5]RU[chinese];B[aa];W[bb])".to_vec(),
    )))
    .await
    .unwrap();
    document(&mut output).await;
    let baseline = validation_work(&actor);
    assert_eq!(baseline.1, 0);

    // Use the same batch result entry point as the actor loop. Its existing
    // single-point protocol checks remain; publishing must not replay the graph.
    let results = [NodeId(1), NodeId(2)]
        .into_iter()
        .map(|node| {
            let value = incoming(&mut actor, node, 100);
            ReviewResult {
                node_id: value.node_id,
                key: value.key,
                response: value.response,
            }
        })
        .collect();
    actor.review.progress = Some(WholeGameProgress {
        source: WholeGameSource::Manual,
        completed: 0,
        total: 3,
        visits: 100,
        status: WholeGameStatus::Running,
    });
    within(actor.review_signal(Some(Ok(results)))).await;
    let reviewed = document(&mut output).await;
    assert!(
        reviewed.graph.points[1..]
            .iter()
            .all(|p| p.summary.is_some())
    );
    assert_eq!(validation_work(&actor), baseline);

    for command in [
        Command::Step(1),
        Command::GoTo(NodeId(0)),
        Command::Play(Move::Play(Color::Black, Point { x: 0, y: 0 })),
    ] {
        within(actor.apply(Input::Edit(command))).await.unwrap();
        let navigated = document(&mut output).await;
        assert!(
            navigated.graph.points[1..]
                .iter()
                .all(|p| p.summary.is_some())
        );
        assert_eq!(validation_work(&actor), baseline);
    }
    assert_eq!(actor.document.current(), NodeId(1));

    // A current-node result updates AnalysisUpdated. A non-current result
    // (the board-sync focus) publishes DocumentChanged directly.
    for node in [NodeId(1), NodeId(2)] {
        let value = incoming(&mut actor, node, 200);
        actor.signal(Signal::Snapshot(Box::new(value)));
        if node == actor.document.current() {
            actor.publish_document().unwrap();
        }
        let analyzed = document(&mut output).await;
        let point = analyzed
            .graph
            .points
            .iter()
            .find(|p| p.node_id == node.0)
            .unwrap();
        assert_eq!(point.summary.unwrap().visits, 200);
        assert_eq!(validation_work(&actor), baseline);
    }

    let info = actor.document.game_info().unwrap();
    within(
        actor.apply(Input::Edit(Command::SetGameInfo(document::GameInfoEdit {
            black_name: info.black_name,
            black_rank: info.black_rank,
            white_name: info.white_name,
            white_rank: info.white_rank,
            komi: 6.5,
            rules: "chinese".into(),
            result: info.result,
            date: info.date,
            event: info.event,
        }))),
    )
    .await
    .unwrap();
    let edited = document(&mut output).await;
    assert!(edited.graph.points.iter().all(|p| p.summary.is_none()));
    let after_edit = (baseline.0 + 1, baseline.1 + 2);
    assert_eq!(validation_work(&actor), after_edit);
    actor.publish_document().unwrap();
    document(&mut output).await;
    assert_eq!(validation_work(&actor), after_edit);

    within(actor.apply(Input::Undo)).await.unwrap();
    let restored = document(&mut output).await;
    assert!(
        restored.graph.points[1..]
            .iter()
            .all(|p| p.summary.is_some())
    );
    assert_eq!(validation_work(&actor), (baseline.0 + 2, baseline.1 + 4));
    within(actor.apply(Input::Redo)).await.unwrap();
    let redone = document(&mut output).await;
    assert!(redone.graph.points.iter().all(|p| p.summary.is_none()));
    assert_eq!(validation_work(&actor), (baseline.0 + 3, baseline.1 + 6));

    // Creating a new child is content, unlike advancing to an existing child.
    within(actor.apply(Input::Edit(Command::Play(Move::Play(
        Color::White,
        Point { x: 2, y: 2 },
    )))))
    .await
    .unwrap();
    document(&mut output).await;
    assert_eq!(validation_work(&actor), (baseline.0 + 4, baseline.1 + 8));
}
