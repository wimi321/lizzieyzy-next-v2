#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use app_core::{AppEvent, AppSubscription, tencent::TencentQuery};
use common::within;

async fn move_count(events: &mut AppSubscription, expected: u64) -> usize {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { revision, view } = events.recv().await.unwrap() {
                assert_eq!(revision, expected);
                assert!(view.position_error.is_none());
                return view.board.unwrap().move_number;
            }
        }
    })
    .await
}

// Opt in with TENCENT_ACCOUNT. Only TXWQFetchChessList and TXWQFetchChess are
// contacted: one first page and one SGF, without printing account identities.
#[tokio::test]
#[ignore = "manual Tencent acceptance; requires TENCENT_ACCOUNT and network access"]
async fn live_account_list_download_and_open() {
    let account =
        std::env::var("TENCENT_ACCOUNT").expect("set TENCENT_ACCOUNT to a public nickname or UID");
    assert!(
        !account.trim().is_empty(),
        "TENCENT_ACCOUNT must not be empty"
    );
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::create_dir_all(&target).unwrap();
    let directory = tempfile::Builder::new()
        .prefix("tencent-live-")
        .tempdir_in(target)
        .unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let page = match app.tencent_search(TencentQuery(account)).await {
        Ok(page) => page,
        Err(_) => panic!("Tencent first-page request failed"),
    };
    let game = page.games.first().expect("public account has no games");
    let revision = match app.tencent_open_game(game.id.clone()).await {
        Ok(revision) => revision,
        Err(_) => panic!("Tencent SGF download or open failed"),
    };
    move_count(&mut events, revision).await;
    let revision = within(app.step(i32::MAX)).await.unwrap();
    let moves = move_count(&mut events, revision).await;
    assert_eq!(moves, game.move_count as usize);
    println!(
        "Tencent live PASS page_count={} moves={moves}",
        page.games.len()
    );
    within(app.shutdown()).await.unwrap();
}
