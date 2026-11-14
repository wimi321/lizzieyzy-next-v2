#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
#[path = "common/online.rs"]
mod online;

use app_core::{
    AppConfig, AppCore, AppError, AppSubscription, Settings,
    tencent::{TencentClient, TencentError, TencentQuery},
};
use common::within;
use online::{Reply, Server, document_event};
use serde_json::json;
use std::path::Path;
use tokio::{net::TcpListener, sync::oneshot};

const EMPTY_PAGE: &str = r#"{"result":0,"ret":0,"chesslist":[]}"#;

fn query(value: &str) -> TencentQuery {
    TencentQuery(value.into())
}

fn page(id: &str) -> String {
    json!({"result":0,"ret":0,"chesslist":[{
        "chessid":id,"gamestarttime":1234567890,
        "blackuid":1001,"whiteuid":1002,"blacknick":"样本甲","whitenick":"样本乙",
        "blackdan":24,"whitedan":23,"movenum":2,"winner":1,"point":-1,"rule":1
    }]})
    .to_string()
}

async fn spawn(directory: &Path, client: TencentClient) -> (AppCore, AppSubscription) {
    within(app_core::spawn_with_config(
        directory,
        AppConfig {
            tencent_client: Some(client),
            ..AppConfig::default()
        },
    ))
    .await
    .unwrap()
}

#[tokio::test]
async fn successful_search_history_is_trimmed_deduplicated_bounded_persisted_and_clearable() {
    let server = Server::start((0..13).map(|_| Reply::body(EMPTY_PAGE)).collect()).await;
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = spawn(directory.path(), server.tencent_client()).await;
    for index in 1..=12 {
        within(app.tencent_search(query(&format!("sample-{index}"))))
            .await
            .unwrap();
    }
    within(app.tencent_search(query("  sample-7 \t")))
        .await
        .unwrap();
    let settings = within(app.settings()).await.unwrap();
    let actual: Vec<_> = settings
        .tencent_history
        .iter()
        .map(|value| value.0.as_str())
        .collect();
    assert_eq!(
        actual,
        [
            "sample-7",
            "sample-12",
            "sample-11",
            "sample-10",
            "sample-9",
            "sample-8",
            "sample-6",
            "sample-5",
            "sample-4",
            "sample-3"
        ]
    );
    assert_eq!(
        within(Settings::load(directory.path())).await.unwrap(),
        settings
    );
    assert_eq!(server.finish().await.len(), 13);
    within(app.shutdown()).await.unwrap();

    let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    assert_eq!(within(app.settings()).await.unwrap(), settings);
    within(app.tencent_clear_history()).await.unwrap();
    assert!(
        within(app.settings())
            .await
            .unwrap()
            .tencent_history
            .is_empty()
    );
    assert!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .tencent_history
            .is_empty()
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn short_nonempty_pages_continue_until_empty_without_changing_search_history() {
    let server = Server::start(vec![
        Reply::body(page("game-a")),
        Reply::body(page("game-b")),
        Reply::body(EMPTY_PAGE),
    ])
    .await;
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = spawn(directory.path(), server.tencent_client()).await;
    let first = within(app.tencent_search(query("1001"))).await.unwrap();
    assert_eq!(first.games.len(), 1);
    assert_eq!(first.next_cursor.as_deref(), Some("game-a"));
    let second = within(app.tencent_next_page(query("1001"), first.next_cursor.unwrap()))
        .await
        .unwrap();
    assert_eq!(second.games.len(), 1);
    assert_eq!(second.next_cursor.as_deref(), Some("game-b"));
    let last = within(app.tencent_next_page(query("1001"), second.next_cursor.unwrap()))
        .await
        .unwrap();
    assert!(last.games.is_empty());
    assert!(last.next_cursor.is_none());
    assert_eq!(
        within(app.settings()).await.unwrap().tencent_history,
        [query("1001")]
    );
    let requests = server.finish().await;
    assert!(requests[0].contains("username=1001"));
    assert!(requests[0].contains("srcuid=1001"));
    assert!(requests[0].contains("lastCode=0"));
    assert!(requests[1].contains("lastCode=game-a"));
    assert!(requests[2].contains("lastCode=game-b"));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn failed_search_and_repeated_cursor_do_not_update_history_or_the_document() {
    let server = Server::start(vec![
        Reply::body(r#"{"result":8,"resultstr":"unavailable"}"#),
        Reply::body(page("repeat")),
    ])
    .await;
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = spawn(directory.path(), server.tencent_client()).await;
    let revision = within(app.open_bytes(b"(;SZ[9]C[keep];B[aa])".to_vec()))
        .await
        .unwrap();
    document_event(&mut events, revision).await;
    assert!(matches!(
        within(app.tencent_search(query("sample"))).await,
        Err(AppError::Tencent(TencentError::Api { result: 8, .. }))
    ));
    assert!(matches!(
        within(app.tencent_search(query("  "))).await,
        Err(AppError::Tencent(TencentError::InvalidQuery))
    ));
    assert!(
        within(app.tencent_next_page(query("sample"), "repeat".into()))
            .await
            .is_err()
    );
    assert!(
        within(app.settings())
            .await
            .unwrap()
            .tencent_history
            .is_empty()
    );
    let next_revision = within(app.step(1)).await.unwrap();
    assert_eq!(next_revision, revision + 1);
    assert_eq!(
        document_event(&mut events, next_revision)
            .await
            .board
            .unwrap()
            .move_number,
        1
    );
    assert_eq!(server.finish().await.len(), 2);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn failed_history_save_preserves_memory_and_other_preferences() {
    let server = Server::start(vec![Reply::body(EMPTY_PAGE), Reply::body(EMPTY_PAGE)]).await;
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = spawn(directory.path(), server.tencent_client()).await;
    within(app.set_pondering(false)).await.unwrap();
    within(app.tencent_search(query("sample"))).await.unwrap();
    let before = within(app.settings()).await.unwrap();
    let settings_path = directory.path().join("settings.json");
    std::fs::rename(&settings_path, directory.path().join("settings.backup")).unwrap();
    std::fs::create_dir(&settings_path).unwrap();
    assert!(matches!(
        within(app.tencent_search(query("another"))).await,
        Err(AppError::Settings(_))
    ));
    assert_eq!(within(app.settings()).await.unwrap(), before);
    assert!(matches!(
        within(app.tencent_clear_history()).await,
        Err(AppError::Settings(_))
    ));
    assert_eq!(within(app.settings()).await.unwrap(), before);
    assert_eq!(server.finish().await.len(), 2);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn pending_search_does_not_block_document_or_settings_commands() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = online::tencent_client(&format!("http://{}", listener.local_addr().unwrap()));
    let (arrived, arrival) = oneshot::channel();
    let (release, released) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        online::read_request(&mut socket).await;
        arrived.send(()).unwrap();
        released.await.unwrap();
        online::respond(&mut socket, &Reply::body(EMPTY_PAGE)).await;
    });
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = spawn(directory.path(), client).await;
    let searching = app.clone();
    let search = tokio::spawn(async move { searching.tencent_search(query("sample")).await });
    within(arrival).await.unwrap();
    let revision = within(app.open_bytes(b"(;SZ[9]C[responsive])".to_vec()))
        .await
        .unwrap();
    assert_eq!(
        document_event(&mut events, revision)
            .await
            .comment
            .as_deref(),
        Some("responsive")
    );
    within(app.set_pondering(false)).await.unwrap();
    assert!(
        within(app.settings())
            .await
            .unwrap()
            .tencent_history
            .is_empty()
    );
    release.send(()).unwrap();
    within(search).await.unwrap().unwrap();
    within(server).await.unwrap();
    let saved = within(app.settings()).await.unwrap();
    assert!(!saved.pondering);
    assert_eq!(saved.tencent_history, [query("sample")]);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn download_success_uses_normal_open_and_failures_keep_current_game() {
    let normalized = "(;FF[4]GM[1]CA[UTF-8]SZ[9]PB[样本甲]PW[样本乙];B[aa];W[bb])";
    let server = Server::start(vec![
        Reply::body(json!({"result":0,"chess":format!("\u{feff}\\{normalized}")}).to_string()),
        Reply::body(r#"{"result":9,"resultstr":"not available"}"#),
        Reply::body(r#"{"result":0,"chess":"(;C[unterminated"}"#),
    ])
    .await;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.sgf");
    std::fs::write(&source, b"(;SZ[19]C[old])").unwrap();
    let (app, mut events) = spawn(directory.path(), server.tencent_client()).await;
    within(app.open_path(source)).await.unwrap();
    let revision = within(app.tencent_open_game("game-success".into()))
        .await
        .unwrap();
    let opened = document_event(&mut events, revision).await;
    assert_eq!(opened.file_name, None);
    assert_eq!(opened.file_path, None);
    assert_eq!(opened.game_info.black_name.as_deref(), Some("样本甲"));
    assert_eq!(opened.tree.main_line.len(), 3);
    for id in ["game-api-failure", "game-malformed"] {
        assert!(within(app.tencent_open_game(id.into())).await.is_err());
        assert_eq!(within(app.step(0)).await.unwrap(), revision);
        assert_eq!(within(app.variation_tree()).await.unwrap().nodes.len(), 3);
    }
    let revision = within(app.step(i32::MAX)).await.unwrap();
    assert_eq!(
        document_event(&mut events, revision)
            .await
            .board
            .unwrap()
            .move_number,
        2
    );
    assert!(
        within(app.settings())
            .await
            .unwrap()
            .tencent_history
            .is_empty()
    );
    assert_eq!(server.finish().await.len(), 3);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn recorded_list_and_separate_recorded_sgf_open_through_the_public_app_api() {
    // Recorded 2026-11-14 from TXWQFetchChessList and TXWQFetchChess. The list
    // has Beijing date-time strings in starttime and integer gamestarttime.
    // This older SGF is not in the captured recent pages; the synthetic request
    // ID below deliberately does not claim that the first list row is this game.
    let server = Server::start(vec![
        Reply::body(include_str!("../../../fixtures/tencent/games.json")),
        Reply::body(include_str!("../../../fixtures/tencent/sgf.json")),
    ])
    .await;
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = spawn(directory.path(), server.tencent_client()).await;
    let page = within(app.tencent_search(query("样本棋手910002")))
        .await
        .unwrap();
    assert_eq!(page.games.len(), 5);
    assert!(page.next_cursor.is_some());
    assert_eq!(page.games[0].start_time, Some(1794548917));
    let revision = within(app.tencent_open_game("recorded-detail".into()))
        .await
        .unwrap();
    let opened = document_event(&mut events, revision).await;
    assert_eq!(
        opened.game_info.black_name.as_deref(),
        Some("样本棋手910002")
    );
    assert_eq!(
        opened.game_info.white_name.as_deref(),
        Some("样本棋手920091")
    );
    assert_eq!(opened.game_info.result.as_deref(), Some("B+0.02"));
    assert_eq!(opened.game_info.komi, 7.5);
    assert_eq!(opened.tree.main_line.len(), 80);
    let revision = within(app.step(i32::MAX)).await.unwrap();
    let last = document_event(&mut events, revision).await;
    assert!(last.position_error.is_none());
    assert_eq!(last.board.unwrap().move_number, 79);
    let requests = server.finish().await;
    assert!(requests[0].starts_with("GET /tencent-list?"));
    assert!(requests[1].starts_with("GET /tencent-sgf?chessid=recorded-detail "));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn old_settings_and_missing_optional_directories_do_not_prevent_startup() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("settings.json"),
        br#"{"version":1,"pondering":false}"#,
    )
    .unwrap();
    let (app, _events) = within(app_core::spawn_with_config(
        directory.path(),
        AppConfig {
            games_directory: Some(directory.path().join("missing").join("games")),
            ..AppConfig::default()
        },
    ))
    .await
    .unwrap();
    assert!(
        within(app.settings())
            .await
            .unwrap()
            .tencent_history
            .is_empty()
    );
    assert!(!directory.path().join("missing").exists());
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn loading_hand_edited_history_trims_deduplicates_and_applies_the_limit() {
    let directory = tempfile::tempdir().unwrap();
    let mut history = vec![
        " sample-0 ".to_owned(),
        "sample-0".to_owned(),
        " ".to_owned(),
    ];
    history.extend((1..=12).map(|index| format!("sample-{index}")));
    std::fs::write(
        directory.path().join("settings.json"),
        json!({"version":1,"tencent_history":history}).to_string(),
    )
    .unwrap();
    let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    let history = within(app.settings()).await.unwrap().tencent_history;
    let expected: Vec<_> = (0..10)
        .map(|index| query(&format!("sample-{index}")))
        .collect();
    assert_eq!(history, expected);
    within(app.shutdown()).await.unwrap();
}
