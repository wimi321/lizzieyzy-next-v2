#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AppEvent, AppSubscription, DocumentView, GameInfoEdit, GameInfoView};
use std::{future::Future, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("game information operation exceeded safety deadline")
}

async fn document(events: &mut AppSubscription) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = events.recv().await.unwrap() {
                return view;
            }
        }
    })
    .await
}

fn current_edit(info: &GameInfoView) -> GameInfoEdit {
    GameInfoEdit {
        black_name: info.black_name.clone(),
        black_rank: info.black_rank.clone(),
        white_name: info.white_name.clone(),
        white_rank: info.white_rank.clone(),
        komi: info.komi,
        rules: info.rules_preset.clone(),
        result: info.result.clone(),
        date: info.date.clone(),
        event: info.event.clone(),
    }
}

#[tokio::test]
async fn unchanged_fox_information_preserves_revision_modified_and_edit_history() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let source = b"(;SZ[19]AP[foxwq]KM[375]RU[Chinese]PB[black]PW[white];B[aa];W[bb])";
    let opened_revision = within(app.open_bytes(source.to_vec())).await.unwrap();
    let opened = document(&mut events).await;
    let unchanged = current_edit(&opened.game_info);
    assert_eq!(unchanged.komi, 7.5);
    assert_eq!(unchanged.rules, "chinese");
    assert_eq!(
        within(app.set_game_info(unchanged.clone())).await.unwrap(),
        opened_revision
    );
    assert!(matches!(
        within(app.undo()).await,
        Err(app_core::AppError::Document(
            document::DocError::NothingToUndo
        ))
    ));
    within(app.step(1)).await.unwrap();
    let navigated = document(&mut events).await;
    assert!(!navigated.modified);
    assert_eq!(navigated.game_info, opened.game_info);

    let edited_revision = within(app.set_comment("unsaved comment".into()))
        .await
        .unwrap();
    assert!(document(&mut events).await.modified);
    assert_eq!(
        within(app.set_game_info(unchanged.clone())).await.unwrap(),
        edited_revision
    );
    within(app.step(1)).await.unwrap();
    assert!(document(&mut events).await.modified);
    let undone_revision = within(app.undo()).await.unwrap();
    assert!(!document(&mut events).await.modified);
    assert_eq!(
        within(app.set_game_info(unchanged)).await.unwrap(),
        undone_revision
    );
    within(app.redo()).await.unwrap();
    let redone = document(&mut events).await;
    assert!(redone.modified);
    assert_eq!(redone.comment.as_deref(), Some("unsaved comment"));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn name_only_edit_preserves_raw_fox_metadata_through_save_and_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let source = b"(;SZ[19]AP[foxwq]KM[375][ignored]KM[100]RU[Chinese][ignored]RU[Japanese]PB[black][ignored]PB[duplicate]KO[unused]LZKOMI[stale];B[aa])";
    within(app.open_bytes(source.to_vec())).await.unwrap();
    let opened = document(&mut events).await;
    let mut changed = current_edit(&opened.game_info);
    changed.black_name = Some("renamed".into());
    within(app.set_game_info(changed)).await.unwrap();
    let edited = document(&mut events).await;
    assert!(edited.modified);
    assert_eq!(edited.game_info.komi, 7.5);
    assert_eq!(edited.game_info.rules.as_deref(), Some("Chinese"));
    assert_eq!(edited.game_info.rules_preset, "chinese");
    within(app.undo()).await.unwrap();
    assert!(!document(&mut events).await.modified);
    within(app.redo()).await.unwrap();
    assert!(document(&mut events).await.modified);

    let target = directory.path().join("name-edit.sgf");
    within(app.save(&target)).await.unwrap();
    assert!(!document(&mut events).await.modified);
    let saved = sgf::parse(&std::fs::read(&target).unwrap()).unwrap();
    let original = sgf::parse(source).unwrap();
    let unchanged_properties = |game: &sgf::GameTree| {
        game.sequence[0]
            .properties
            .iter()
            .filter(|property| !matches!(property.identifier.as_str(), "PB" | "CA"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        unchanged_properties(&saved.games[0]),
        unchanged_properties(&original.games[0])
    );
    within(app.open_path(&target)).await.unwrap();
    let reopened = document(&mut events).await;
    assert_eq!(reopened.game_info, edited.game_info);
    assert!(!reopened.modified);
    within(app.shutdown()).await.unwrap();
}
