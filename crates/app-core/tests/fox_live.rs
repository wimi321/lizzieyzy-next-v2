#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AppEvent, AppSubscription, DocumentView, fox::FoxQuery};
use std::time::Duration;

async fn document_event(events: &mut AppSubscription, expected: u64) -> DocumentView {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let AppEvent::DocumentChanged { revision, view } = events.recv().await.unwrap() {
                assert_eq!(revision, expected);
                return view;
            }
        }
    })
    .await
    .expect("local document event deadline exceeded")
}

// The nickname occurs in Java GetFoxRequestTest. This opt-in check uses the
// production AppCore/FoxClient path, including all CGI/H5 fallback decisions.
// It writes only into a temporary settings directory and never records public
// account identities or unredacted network responses into repository fixtures.
#[tokio::test]
#[ignore = "manual acceptance: real Fox nickname -> list -> download -> open"]
async fn live_nickname_list_download_and_open() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = app_core::spawn(directory.path()).await.unwrap();
    let result = app
        .fox_search(FoxQuery::Nickname("廿月廿".into()))
        .await
        .expect("real nickname and first-page request must succeed");
    let game = result
        .page
        .games
        .first()
        .expect("public game list is nonempty");
    println!(
        "Fox live list_count={} selected_game_id={} listed_moves={}",
        result.page.games.len(),
        game.id,
        game.move_count
    );
    let revision = app
        .fox_open_game(game.id.clone())
        .await
        .expect("production CGI/H5 fallback and SGF open must succeed");
    let opened = document_event(&mut events, revision).await;
    assert!(opened.position_error.is_none());
    let last = *opened.tree.main_line.last().unwrap();
    let revision = app.go_to(last).await.unwrap();
    let final_view = document_event(&mut events, revision).await;
    assert!(final_view.position_error.is_none());
    let moves = final_view.board.unwrap().move_number;
    assert_eq!(moves, game.move_count as usize);

    let path = directory.path().join("downloaded.sgf");
    app.save(&path).await.unwrap();
    let collection = sgf::parse(&std::fs::read(path).unwrap()).unwrap();
    let document = document::GameDocument::from_sgf(&collection.games[0]).unwrap();
    let komi = document.position_at(document.root()).unwrap().komi;
    assert_eq!(komi, 7.5);
    println!(
        "Fox live PASS list_count={} game_id={} moves={} komi={}",
        result.page.games.len(),
        game.id,
        moves,
        komi
    );
    app.shutdown().await.unwrap();
}
