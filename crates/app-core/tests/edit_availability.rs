#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AppEvent, AppSubscription, DocumentView, NewGameOptions};
use go_core::{Color, Move, Point};
use std::{future::Future, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("edit availability safety deadline")
}

async fn document(events: &mut AppSubscription, expected: u64) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { revision, view } = events.recv().await.unwrap() {
                assert_eq!(revision, expected);
                return view;
            }
        }
    })
    .await
}

fn availability(view: &DocumentView) -> (bool, bool) {
    (view.can_undo, view.can_redo)
}

#[tokio::test]
async fn document_view_tracks_edit_history_through_navigation_save_and_document_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    assert_eq!(
        availability(&document(&mut events, 0).await),
        (false, false)
    );
    let opened = within(app.open_bytes(b"(;SZ[9];B[aa];W[bb])".to_vec()))
        .await
        .unwrap();
    assert_eq!(
        availability(&document(&mut events, opened).await),
        (false, false)
    );
    let edited = within(app.set_comment("first edit".into())).await.unwrap();
    assert_eq!(
        availability(&document(&mut events, edited).await),
        (true, false)
    );
    let undone = within(app.undo()).await.unwrap();
    assert_eq!(
        availability(&document(&mut events, undone).await),
        (false, true)
    );

    let stepped = within(app.step(1)).await.unwrap();
    assert_eq!(
        availability(&document(&mut events, stepped).await),
        (false, true)
    );
    let existing = within(app.play(Move::Play(Color::White, Point { x: 1, y: 1 })))
        .await
        .unwrap();
    assert_eq!(
        availability(&document(&mut events, existing).await),
        (false, true)
    );
    let navigated = within(app.go_to(0)).await.unwrap();
    assert_eq!(
        availability(&document(&mut events, navigated).await),
        (false, true)
    );
    let redone = within(app.redo()).await.unwrap();
    assert_eq!(
        availability(&document(&mut events, redone).await),
        (true, false)
    );

    let second_edit = within(app.set_comment("second edit".into())).await.unwrap();
    document(&mut events, second_edit).await;
    let undone = within(app.undo()).await.unwrap();
    assert_eq!(
        availability(&document(&mut events, undone).await),
        (true, true)
    );
    within(app.save(directory.path().join("saved.sgf")))
        .await
        .unwrap();
    let saved = document(&mut events, undone).await;
    assert_eq!(availability(&saved), (true, true));
    assert!(!saved.modified);

    let new = within(app.new_game(NewGameOptions::default()))
        .await
        .unwrap();
    assert_eq!(
        availability(&document(&mut events, new).await),
        (false, false)
    );
    let played = within(app.play(Move::Play(Color::Black, Point { x: 0, y: 0 })))
        .await
        .unwrap();
    assert_eq!(
        availability(&document(&mut events, played).await),
        (true, false)
    );
    let opened = within(app.open_path(directory.path().join("saved.sgf")))
        .await
        .unwrap();
    assert_eq!(
        availability(&document(&mut events, opened).await),
        (false, false)
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn game_info_view_preserves_raw_rules_and_exposes_the_replay_preset() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let cases = [
        (Some(" Chinese "), "chinese"),
        (Some("Japanese"), "japanese"),
        (Some(" KOREAN"), "korean"),
        (Some("AGA"), "aga"),
        (Some("Tromp-Taylor"), "tromp-taylor"),
        (Some("NEW-ZEALAND "), "new-zealand"),
        (Some("unrecognized rules"), "chinese"),
        (None, "chinese"),
    ];
    for (raw, expected) in cases {
        let source = raw.map_or_else(|| "(;SZ[9])".to_owned(), |raw| format!("(;SZ[9]RU[{raw}])"));
        let parsed = sgf::parse(source.as_bytes()).unwrap();
        let reference = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
        let preset = reference.position_at(reference.root()).unwrap().rules;
        let revision = within(app.open_bytes(source.into_bytes())).await.unwrap();
        let view = document(&mut events, revision).await;
        assert_eq!(view.game_info.rules.as_deref(), raw);
        assert_eq!(view.game_info.rules_preset, expected);
        assert_eq!(view.game_info.rules_preset, preset.as_katago_str());
        let wire = serde_json::to_value(&view).unwrap();
        assert_eq!(wire["game_info"]["rules_preset"], expected);
        assert_eq!(wire["can_undo"], false);
        assert_eq!(wire["can_redo"], false);
    }
    within(app.shutdown()).await.unwrap();
}

#[test]
fn old_document_dto_defaults_new_fields_without_interpreting_raw_rules_in_the_client() {
    let parsed = sgf::parse(b"(;RU[Japanese])").unwrap();
    let source = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
    let mut old_wire =
        serde_json::to_value(DocumentView::from_document(&source, 0).unwrap()).unwrap();
    old_wire.as_object_mut().unwrap().remove("can_undo");
    old_wire.as_object_mut().unwrap().remove("can_redo");
    old_wire["game_info"]
        .as_object_mut()
        .unwrap()
        .remove("rules_preset");
    let decoded: DocumentView = serde_json::from_value(old_wire).unwrap();
    assert_eq!(availability(&decoded), (false, false));
    assert_eq!(decoded.game_info.rules.as_deref(), Some("Japanese"));
    assert_eq!(decoded.game_info.rules_preset, "chinese");
}
