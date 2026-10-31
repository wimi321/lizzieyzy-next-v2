#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use document::{Command, GameDocument};
use go_core::{BoardSize, Color, CoordinateError};
use serde_json::json;

fn document(bytes: &[u8]) -> GameDocument {
    let collection = sgf::parse(bytes).unwrap();
    GameDocument::from_sgf(&collection.games[0]).unwrap()
}

#[test]
fn board_and_tree_views_follow_captures_passes_and_variations() {
    let mut doc = document(b"(;SZ[3]AB[ba]AW[aa]PL[B];B[ab](;W[])(;W[cc]))");
    let root = DocumentView::from_document(&doc, 0).unwrap();
    assert_eq!(root.tree.main_line, [0, 1, 2]);
    assert_eq!(root.tree.variation_count, 1);
    let board = root.board.unwrap();
    assert_eq!((board.width, board.height), (3, 3));
    assert_eq!(board.move_number, 0);
    assert_eq!(board.last_move, None);
    assert_eq!(board.stones[0], Some(ColorView::White));
    assert_eq!(board.stones[1], Some(ColorView::Black));

    doc.apply(Command::Step(1)).unwrap();
    let captured = DocumentView::from_document(&doc, 0).unwrap();
    assert_eq!(captured.tree.current, 1);
    assert_eq!(captured.tree.variation_count, 2);
    let board = captured.board.unwrap();
    assert_eq!(board.stones.len(), 9);
    assert_eq!(board.stones[0], None);
    assert_eq!(board.stones[3], Some(ColorView::Black));
    assert_eq!(board.captured_black, 1);
    assert_eq!(board.captured_white, 0);
    assert_eq!(board.move_number, 1);
    assert_eq!(board.to_play, ColorView::White);
    assert_eq!(
        board.last_move,
        Some(MoveView::Play {
            color: ColorView::Black,
            point: PointView { x: 0, y: 1 },
        })
    );

    doc.apply(Command::Step(1)).unwrap();
    let passed = DocumentView::from_document(&doc, 0).unwrap().board.unwrap();
    assert_eq!(passed.move_number, 2);
    assert_eq!(passed.to_play, ColorView::Black);
    assert_eq!(passed.captured_black, 1);
    assert_eq!(
        passed.last_move,
        Some(MoveView::Pass {
            color: ColorView::White
        })
    );
}

#[test]
fn invalid_positions_keep_tree_comments_and_document_warnings() {
    let mut doc = document(b"(;SZ[9]KM[broken]RU[unknown];B[aa];W[aa]C[inspect me]PL[?])");
    doc.apply(Command::Step(2)).unwrap();
    let view = DocumentView::from_document(&doc, 0).unwrap();
    assert_eq!(view.tree.current, 2);
    assert_eq!(view.tree.main_line, [0, 1, 2]);
    assert_eq!(view.graph.current_index, 2);
    assert_eq!(
        view.graph
            .points
            .iter()
            .map(|point| (point.node_id, point.move_number, point.summary))
            .collect::<Vec<_>>(),
        [(0, 0, None), (1, 1, None), (2, 2, None)]
    );
    assert_eq!(view.comment.as_deref(), Some("inspect me"));
    assert!(view.board.is_none());
    assert!(view.position_error.unwrap().contains("node"));
    assert_eq!(
        view.warnings,
        doc.warnings()
            .into_iter()
            .map(DocumentWarningView::from)
            .collect::<Vec<_>>()
    );
    assert!(view.warnings.contains(&DocumentWarningView {
        node: 0,
        property: "KM".into(),
        kind: WarningKindView::InvalidValue {
            fallback: "7.5".into()
        },
    }));
    assert!(view.warnings.contains(&DocumentWarningView {
        node: 0,
        property: "RU".into(),
        kind: WarningKindView::UnknownRules {
            fallback: "chinese".into()
        },
    }));
    assert!(view.warnings.contains(&DocumentWarningView {
        node: 2,
        property: "PL".into(),
        kind: WarningKindView::IgnoredPlayer,
    }));
}

#[test]
fn white_to_play_analysis_view_preserves_black_values_and_candidate_order() {
    let response = serde_json::from_value(json!({
        "id": "test-query", "turnNumber": 3, "isDuringSearch": false,
        "rootInfo": {
            "visits": 43, "winrate": 0.73, "scoreLead": 5.2,
            "utility": 0.45, "currentPlayer": "W"
        },
        "moveInfos": [
            {"move": "C3", "visits": 31, "winrate": 0.69,
             "scoreLead": 4.5, "prior": 0.33, "order": 0, "pv": ["C3", "pass"]},
            {"move": "pass", "visits": 12, "winrate": 0.8,
             "scoreLead": 8.1, "prior": 0.1, "order": 1, "pv": ["pass"]}
        ],
        "ownership": [-0.7, 0.8, 0.0]
    }))
    .unwrap();
    let expected = AnalysisView {
        node_id: 3,
        revision: 12,
        winrate: 0.73,
        score_lead: 5.2,
        visits: 43,
        visits_per_second: Some(1250.5),
        summary: AnalysisSummary {
            winrate: 0.73,
            score_lead: 5.2,
            visits: 43,
        },
        candidates: vec![
            CandidateView {
                coordinate: "C3".into(),
                point: Some(PointView { x: 2, y: 16 }),
                visits: 31,
                winrate: 0.69,
                score_lead: 4.5,
                prior: 0.33,
                order: 0,
                pv: vec![
                    MoveView::Play {
                        color: ColorView::White,
                        point: PointView { x: 2, y: 16 },
                    },
                    MoveView::Pass {
                        color: ColorView::Black,
                    },
                ],
            },
            CandidateView {
                coordinate: "pass".into(),
                point: None,
                visits: 12,
                winrate: 0.8,
                score_lead: 8.1,
                prior: 0.1,
                order: 1,
                pv: vec![MoveView::Pass {
                    color: ColorView::White,
                }],
            },
        ],
        ownership: Some(vec![-0.7, 0.8, 0.0]),
        is_during_search: false,
        source: AnalysisSource::Live,
    };
    let snapshot = analysis::AnalysisSnapshot {
        node_id: document::NodeId(3),
        revision: 12,
        key: document::AnalysisKey([7; 32]),
        response,
        source: analysis::SnapshotSource::Live,
        visits_per_second: Some(1250.5),
    };
    let size = BoardSize::new(19, 19).unwrap();
    assert_eq!(
        AnalysisView::from_snapshot(snapshot.clone(), size, Color::White).unwrap(),
        expected
    );
    let mut cached = snapshot;
    cached.source = analysis::SnapshotSource::Cache;
    cached.visits_per_second = None;
    let view = AnalysisView::from_snapshot(cached, size, Color::White).unwrap();
    assert_eq!(view.source, AnalysisSource::Cache);
    assert_eq!(view.winrate, expected.winrate);
    assert_eq!(view.score_lead, expected.score_lead);
    assert_eq!(view.summary, expected.summary);
    assert_eq!(view.visits_per_second, None);
}

fn rectangular_snapshot() -> analysis::AnalysisSnapshot {
    let coordinates = ["A13", "J9", "E1", "A10", "pass"];
    let ownership: Vec<_> = (0..117)
        .map(|index| (f64::from(index) - 58.0) / 58.0)
        .collect();
    let candidates: Vec<_> = coordinates
        .iter()
        .enumerate()
        .map(|(order, coordinate)| {
            json!({
                "move": coordinate, "visits": 10, "winrate": 0.7, "scoreLead": 4.0,
                "prior": 0.2, "order": order, "pv": [coordinate, "pass", "J9"]
            })
        })
        .collect();
    analysis::AnalysisSnapshot {
        node_id: document::NodeId(0),
        revision: 4,
        key: document::AnalysisKey([4; 32]),
        response: serde_json::from_value(json!({
            "id": "rectangular", "turnNumber": 0, "isDuringSearch": false,
            "rootInfo": {
                "visits": 50, "winrate": 0.7, "scoreLead": 4.0,
                "utility": 0.2, "currentPlayer": "W"
            },
            "moveInfos": candidates, "ownership": ownership
        }))
        .unwrap(),
        source: analysis::SnapshotSource::Live,
        visits_per_second: None,
    }
}

#[test]
fn rectangular_board_ownership_candidates_and_pv_share_top_left_coordinates() {
    let doc = document(b"(;SZ[9:13]AB[aa][ie][em]AW[ad]PL[W])");
    let board = DocumentView::from_document(&doc, 0).unwrap().board.unwrap();
    assert_eq!((board.width, board.height), (9, 13));
    let snapshot = rectangular_snapshot();
    let raw_ownership = snapshot.response.ownership.clone().unwrap();
    let view = AnalysisView::from_snapshot(snapshot, BoardSize::new(9, 13).unwrap(), Color::White)
        .unwrap();
    let ownership = view.ownership.as_ref().unwrap();
    assert_eq!(ownership, &raw_ownership);
    assert_eq!(ownership.len(), board.stones.len());
    for (candidate, (point, stone)) in view.candidates.iter().zip([
        (PointView { x: 0, y: 0 }, ColorView::Black),
        (PointView { x: 8, y: 4 }, ColorView::Black),
        (PointView { x: 4, y: 12 }, ColorView::Black),
        (PointView { x: 0, y: 3 }, ColorView::White),
    ]) {
        assert_eq!(candidate.point, Some(point));
        let index = usize::from(point.y) * usize::from(board.width) + usize::from(point.x);
        assert_eq!(board.stones[index], Some(stone));
        assert_eq!(ownership[index], raw_ownership[index]);
        assert_eq!(
            candidate.pv,
            [
                MoveView::Play {
                    color: ColorView::White,
                    point
                },
                MoveView::Pass {
                    color: ColorView::Black
                },
                MoveView::Play {
                    color: ColorView::White,
                    point: PointView { x: 8, y: 4 }
                },
            ]
        );
    }
    let pass = &view.candidates[4];
    assert_eq!(pass.coordinate, "pass");
    assert_eq!(pass.point, None);
    assert_eq!(
        pass.pv[0],
        MoveView::Pass {
            color: ColorView::White
        }
    );
    assert_eq!(
        pass.pv[1],
        MoveView::Pass {
            color: ColorView::Black
        }
    );

    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(
        json["summary"],
        json!({"winrate": 0.7, "score_lead": 4.0, "visits": 50})
    );
    assert_eq!(json["candidates"][1]["point"], json!({"x": 8, "y": 4}));
    assert_eq!(
        json["candidates"][1]["pv"][0],
        json!({
            "type": "Play", "data": {"color": "White", "point": {"x": 8, "y": 4}}
        })
    );
    assert_eq!(json["candidates"][4]["point"], serde_json::Value::Null);
    assert_eq!(serde_json::from_value::<AnalysisView>(json).unwrap(), view);
}

#[test]
fn pv_colors_start_at_the_focus_player_and_alternate_including_passes() {
    for (to_play, first, second) in [
        (Color::Black, ColorView::Black, ColorView::White),
        (Color::White, ColorView::White, ColorView::Black),
    ] {
        let view = AnalysisView::from_snapshot(
            rectangular_snapshot(),
            BoardSize::new(9, 13).unwrap(),
            to_play,
        )
        .unwrap();
        assert_eq!(
            view.candidates[0].pv,
            [
                MoveView::Play {
                    color: first,
                    point: PointView { x: 0, y: 0 }
                },
                MoveView::Pass { color: second },
                MoveView::Play {
                    color: first,
                    point: PointView { x: 8, y: 4 }
                },
            ]
        );
        assert_eq!(view.candidates[4].pv[0], MoveView::Pass { color: first });
        assert_eq!(view.candidates[4].pv[1], MoveView::Pass { color: second });
    }
}

#[test]
fn invalid_candidate_or_pv_coordinates_reject_the_entire_view() {
    let size = BoardSize::new(9, 13).unwrap();
    for coordinate in ["I3", "K1", "A14", "", "a0"] {
        let mut candidate = rectangular_snapshot();
        candidate.response.move_infos[1].coordinate = coordinate.into();
        assert_eq!(
            AnalysisView::from_snapshot(candidate, size, Color::White),
            Err(CoordinateError::InvalidGtp(coordinate.into()))
        );
        let mut pv = rectangular_snapshot();
        pv.response.move_infos[1].pv[2] = coordinate.into();
        assert_eq!(
            AnalysisView::from_snapshot(pv, size, Color::White),
            Err(CoordinateError::InvalidGtp(coordinate.into()))
        );
    }
    let mut empty = rectangular_snapshot();
    empty.response.move_infos.clear();
    assert!(matches!(
        AnalysisView::from_snapshot(
            empty,
            BoardSize {
                width: 0,
                height: 13
            },
            Color::White
        ),
        Err(CoordinateError::InvalidBoardSize(_))
    ));
}

#[test]
fn event_serialization_and_typescript_declarations_agree_on_public_shape() {
    let event = AppEvent::DocumentChanged {
        revision: 17,
        view: DocumentView::from_document(&GameDocument::new(), 0).unwrap(),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "DocumentChanged");
    assert_eq!(json["data"]["revision"], 17);
    assert_eq!(
        json["data"]["view"]["graph"],
        json!({
            "points": [{"node_id": 0, "move_number": 0, "summary": null}],
            "current_index": 0
        })
    );
    assert_eq!(serde_json::from_value::<AppEvent>(json).unwrap(), event);
    let declarations = [
        ColorView::decl(),
        PointView::decl(),
        MoveView::decl(),
        BoardView::decl(),
        TreeView::decl(),
        crate::AnalysisSummary::decl(),
        crate::GraphPointView::decl(),
        GraphView::decl(),
        WarningKindView::decl(),
        DocumentWarningView::decl(),
        DocumentView::decl(),
        CandidateView::decl(),
        AnalysisSource::decl(),
        AnalysisView::decl(),
        EngineStatus::decl(),
        AppEvent::decl(),
    ];
    for declaration in declarations {
        assert!(!declaration.is_empty());
        assert!(!declaration.contains("bigint"), "{declaration}");
    }
    assert!(AppEvent::decl().contains("revision: number"));
    assert!(AnalysisView::decl().contains("visits: number"));
    assert!(AnalysisView::decl().contains("visits_per_second?: number | null"));
    assert!(AnalysisView::decl().contains("summary: AnalysisSummary"));
    assert!(CandidateView::decl().contains("point: PointView | null"));
    assert!(CandidateView::decl().contains("pv: Array<MoveView>"));
}

#[test]
fn analysis_serialization_preserves_optional_search_speed() {
    for speed in [None, Some(1234.5)] {
        let mut snapshot = rectangular_snapshot();
        snapshot.visits_per_second = speed;
        let view =
            AnalysisView::from_snapshot(snapshot, BoardSize::new(9, 13).unwrap(), Color::White)
                .unwrap();
        assert_eq!(view.visits_per_second, speed);
        let event = AppEvent::AnalysisUpdated(view);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["data"]["visits_per_second"], json!(speed));
        assert_eq!(serde_json::from_value::<AppEvent>(json).unwrap(), event);
    }
}

#[test]
fn analysis_serialization_preserves_best_summary_separately_from_current_result() {
    let mut view = AnalysisView::from_snapshot(
        rectangular_snapshot(),
        BoardSize::new(9, 13).unwrap(),
        Color::White,
    )
    .unwrap();
    view.summary = AnalysisSummary {
        winrate: 0.9,
        score_lead: 8.0,
        visits: 100,
    };
    let event = AppEvent::AnalysisUpdated(view);
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["data"]["visits"], 50);
    assert_eq!(json["data"]["winrate"], 0.7);
    assert_eq!(json["data"]["score_lead"], 4.0);
    assert_eq!(
        json["data"]["summary"],
        json!({"winrate": 0.9, "score_lead": 8.0, "visits": 100})
    );
    assert_eq!(serde_json::from_value::<AppEvent>(json).unwrap(), event);
}

#[test]
fn engine_status_views_preserve_terminal_error_details() {
    let statuses = [
        engine_session::EngineStatus::Starting,
        engine_session::EngineStatus::Ready {
            version: "1.16.5-fake".into(),
        },
        engine_session::EngineStatus::Restarting {
            attempt: 2,
            reason: "process exited".into(),
        },
        engine_session::EngineStatus::Failed {
            reason: "model is missing".into(),
            stderr_tail: vec!["model.bin.gz: no such file".into()],
        },
        engine_session::EngineStatus::Stopped,
    ];
    let actual: Vec<_> = statuses.into_iter().map(EngineStatus::from).collect();
    assert_eq!(
        actual,
        [
            EngineStatus::Starting,
            EngineStatus::Ready {
                version: "1.16.5-fake".into()
            },
            EngineStatus::Restarting {
                attempt: 2,
                reason: "process exited".into()
            },
            EngineStatus::Failed {
                reason: "model is missing".into(),
                stderr_tail: vec!["model.bin.gz: no such file".into()]
            },
            EngineStatus::Stopped,
        ]
    );
    for status in actual {
        let event = AppEvent::EngineStatusChanged(status);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(serde_json::from_value::<AppEvent>(json).unwrap(), event);
    }
}
