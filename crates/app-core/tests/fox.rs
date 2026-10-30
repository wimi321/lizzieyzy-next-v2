#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AppConfig, AppCore, AppError, AppEvent, AppSubscription, DocumentView, Settings,
    fox::{FoxClient, FoxEndpoints, FoxError, FoxQuery},
};
use std::{future::Future, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

const EMPTY_PAGE: &str = r#"{"result":0,"chesslist":[]}"#;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("operation exceeded its safety deadline")
}

fn client(address: std::net::SocketAddr) -> FoxClient {
    client_with_downloads(address, false)
}

fn client_with_downloads(address: std::net::SocketAddr, fallback: bool) -> FoxClient {
    let base = format!("http://{address}");
    FoxClient::with_endpoints(
        FoxEndpoints {
            user_url: format!("{base}/user"),
            list_url: format!("{base}/games"),
            sgf_urls: if fallback {
                vec![
                    format!("{base}/cgi"),
                    format!("{base}/cgi2"),
                    format!("{base}/sgf"),
                ]
            } else {
                vec![format!("{base}/sgf")]
            },
        },
        Duration::from_secs(4),
    )
    .unwrap()
}

async fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let n = stream.read(&mut buffer).await.unwrap();
        assert_ne!(n, 0, "incomplete HTTP request");
        request.extend_from_slice(&buffer[..n]);
        if let Some(offset) = request.windows(4).position(|v| v == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..offset]);
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap_or(0);
            if request.len() >= offset + 4 + length {
                return String::from_utf8(request).unwrap();
            }
        }
    }
}

async fn respond(stream: &mut TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await.unwrap();
    stream.shutdown().await.unwrap();
}

async fn replay(bodies: Vec<String>) -> (FoxClient, JoinHandle<Vec<String>>) {
    replay_with_downloads(bodies, false).await
}

async fn replay_with_downloads(
    bodies: Vec<String>,
    fallback: bool,
) -> (FoxClient, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = client_with_downloads(listener.local_addr().unwrap(), fallback);
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for body in bodies {
            let (mut stream, _) = listener.accept().await.unwrap();
            requests.push(read_request(&mut stream).await);
            respond(&mut stream, &body).await;
        }
        requests
    });
    (client, server)
}

async fn spawn(directory: &std::path::Path, client: FoxClient) -> (AppCore, AppSubscription) {
    within(app_core::spawn_with_config(
        directory,
        AppConfig {
            fox_client: Some(client),
            ..AppConfig::default()
        },
    ))
    .await
    .unwrap()
}

async fn document_event(events: &mut AppSubscription, expected: u64) -> DocumentView {
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

#[tokio::test]
async fn recent_accounts_are_deduplicated_bounded_persisted_and_clearable() {
    let (client, server) = replay(vec![EMPTY_PAGE.to_owned(); 13]).await;
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = spawn(directory.path(), client).await;
    for uid in 1..=12 {
        within(app.fox_search(FoxQuery::Uid(uid.to_string())))
            .await
            .unwrap();
    }
    within(app.fox_search(FoxQuery::Uid("7".into())))
        .await
        .unwrap();
    let settings = within(app.settings()).await.unwrap();
    let uids: Vec<_> = settings
        .fox_history
        .iter()
        .map(|a| a.uid.as_str())
        .collect();
    assert_eq!(uids, ["7", "12", "11", "10", "9", "8", "6", "5", "4", "3"]);
    assert_eq!(
        within(Settings::load(directory.path())).await.unwrap(),
        settings
    );
    within(server).await.unwrap();
    within(app.shutdown()).await.unwrap();

    let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    assert_eq!(within(app.settings()).await.unwrap(), settings);
    within(app.fox_clear_history()).await.unwrap();
    assert!(within(app.settings()).await.unwrap().fox_history.is_empty());
    assert!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .fox_history
            .is_empty()
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn fox_history_and_move_rank_updates_preserve_each_other() {
    let (client, server) = replay(vec![EMPTY_PAGE.to_owned()]).await;
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = spawn(directory.path(), client).await;
    let mut rank = app_core::MoveRankSettings {
        mode: app_core::MoveRankEvaluationMode::Score,
        winrate_thresholds: [2.0, 4.0, 8.0, 16.0, 32.0],
        score_thresholds: [1.0, 2.0, 4.0, 8.0, 16.0],
    };
    within(app.set_move_rank_settings(rank.clone()))
        .await
        .unwrap();
    let result = within(app.fox_search(FoxQuery::Uid("123".into())))
        .await
        .unwrap();
    let settings = within(app.settings()).await.unwrap();
    assert_eq!(settings.move_rank, rank);
    assert_eq!(settings.fox_history, std::slice::from_ref(&result.account));

    rank.mode = app_core::MoveRankEvaluationMode::Winrate;
    within(app.set_move_rank_settings(rank.clone()))
        .await
        .unwrap();
    let settings = within(Settings::load(directory.path())).await.unwrap();
    assert_eq!(settings.move_rank, rank);
    assert_eq!(settings.fox_history, [result.account]);
    within(server).await.unwrap();
    within(app.shutdown()).await.unwrap();

    let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    assert_eq!(within(app.settings()).await.unwrap(), settings);
    within(app.fox_clear_history()).await.unwrap();
    let settings = within(app.settings()).await.unwrap();
    assert!(settings.fox_history.is_empty());
    assert_eq!(settings.move_rank, rank);
    assert_eq!(
        within(Settings::load(directory.path())).await.unwrap(),
        settings
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn failed_search_does_not_remember_an_account_or_change_the_document() {
    let (client, server) = replay(vec![r#"{"result":7,"uid":"123"}"#.into()]).await;
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = spawn(directory.path(), client).await;
    let revision = within(app.open_bytes(b"(;SZ[9];B[aa])".to_vec()))
        .await
        .unwrap();
    document_event(&mut events, revision).await;
    assert!(matches!(
        within(app.fox_search(FoxQuery::Nickname("name".into()))).await,
        Err(AppError::Fox(FoxError::Api { code: 7 }))
    ));
    assert!(matches!(
        within(app.fox_search(FoxQuery::Uid("12x".into()))).await,
        Err(AppError::Fox(FoxError::InvalidUid))
    ));
    assert!(within(app.settings()).await.unwrap().fox_history.is_empty());
    assert_eq!(within(app.step(1)).await.unwrap(), revision + 1);
    assert_eq!(
        document_event(&mut events, revision + 1)
            .await
            .board
            .unwrap()
            .move_number,
        1
    );
    within(server).await.unwrap();
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn pending_http_does_not_block_document_or_settings_commands() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = client(listener.local_addr().unwrap());
    let (started, received) = tokio::sync::oneshot::channel();
    let (release, gate) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_request(&mut stream).await;
        started.send(()).unwrap();
        gate.await.unwrap();
        respond(&mut stream, EMPTY_PAGE).await;
    });
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = spawn(directory.path(), client).await;
    let searching = app.clone();
    let search =
        tokio::spawn(async move { searching.fox_search(FoxQuery::Uid("123".into())).await });
    within(received).await.unwrap();
    let revision = within(app.open_bytes(b"(;SZ[9]C[still responsive])".to_vec()))
        .await
        .unwrap();
    assert_eq!(
        document_event(&mut events, revision)
            .await
            .comment
            .as_deref(),
        Some("still responsive")
    );
    assert!(within(app.settings()).await.unwrap().fox_history.is_empty());
    release.send(()).unwrap();
    within(search).await.unwrap().unwrap();
    within(server).await.unwrap();
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn recorded_nickname_search_fallback_and_open_preserve_moves_and_normalize_komi() {
    // Recorded 2026-10-30: QueryUserInfoPanel, YHWQFetchChessList,
    // happyapp/cgi.foxwq TXWQFetchChess and H5 YHWQFetchChess. The five-game
    // replay consistently replaces account identities, keeping wire types.
    let user = include_str!("../../../fixtures/fox/user.json");
    let games = include_str!("../../../fixtures/fox/games.json");
    let sgf_response = include_str!("../../../fixtures/fox/sgf.json");
    let bytes = include_bytes!("../../../fixtures/sgf/fox-recorded-2026-10-30.sgf");
    let (client, server) = replay_with_downloads(
        vec![
            user.into(),
            games.into(),
            include_str!("../../../fixtures/fox/sgf-cgi.json").into(),
            include_str!("../../../fixtures/fox/sgf-cgi2.txt").into(),
            sgf_response.into(),
        ],
        true,
    )
    .await;
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = spawn(directory.path(), client).await;
    let result = within(app.fox_search(FoxQuery::Nickname("样本棋手甲".into())))
        .await
        .unwrap();
    let game = result.page.games.first().unwrap();
    let revision = within(app.fox_open_game(game.id.clone())).await.unwrap();
    let downloaded_view = document_event(&mut events, revision).await;

    let parsed = sgf::parse(bytes).unwrap();
    let tree = &parsed.games[0];
    let root = &tree.sequence[0];
    let property = |name| {
        root.properties
            .iter()
            .find(|property| property.identifier == name)
            .unwrap()
            .values[0]
            .clone()
    };
    assert_eq!(property("PB"), "样本棋手甲");
    assert_eq!(property("PW"), "样本棋手乙");
    assert_eq!(property("RE"), "B+R");
    assert_eq!(property("KM"), "375");
    assert_eq!(game.move_count, 177);
    assert_eq!(game.result.winner, Some(app_core::fox::FoxColor::Black));
    assert_eq!(game.result.kind, app_core::fox::FoxResultKind::Resignation);
    assert_eq!(property("PB"), game.black.nickname);
    assert_eq!(property("PW"), game.white.nickname);
    let document = document::GameDocument::from_sgf(tree).unwrap();
    assert_eq!(document.position_at(document.root()).unwrap().komi, 7.5);
    let expected = DocumentView::from_document(&document, revision).unwrap();
    assert_eq!(downloaded_view, expected);
    let last = *downloaded_view.tree.main_line.last().unwrap();
    let final_revision = within(app.go_to(last)).await.unwrap();
    let final_view = document_event(&mut events, final_revision).await;
    assert!(final_view.position_error.is_none());
    assert_eq!(final_view.tree.structure_revision, revision);
    assert_eq!(
        final_view.board.unwrap().move_number,
        game.move_count as usize
    );

    let saved = directory.path().join("fox.sgf");
    within(app.save(&saved)).await.unwrap();
    let saved_bytes = std::fs::read(saved).unwrap();
    let saved_tree = sgf::parse(&saved_bytes).unwrap();
    let saved_document = document::GameDocument::from_sgf(&saved_tree.games[0]).unwrap();
    assert_eq!(
        saved_document
            .position_at(saved_document.root())
            .unwrap()
            .komi,
        7.5
    );
    assert_eq!(
        saved_tree.games[0].sequence[0]
            .properties
            .iter()
            .find(|property| property.identifier == "KM")
            .unwrap()
            .values,
        ["375"]
    );
    let local_revision = within(app.open_bytes(saved_bytes)).await.unwrap();
    // Reopening identical SGF creates a new document namespace; its tree
    // version advances even though the board and tree contents are unchanged.
    assert!(local_revision > revision);
    assert_eq!(
        document_event(&mut events, local_revision).await,
        DocumentView::from_document(&document, local_revision).unwrap()
    );
    assert_eq!(
        within(app.settings()).await.unwrap().fox_history,
        [result.account]
    );
    let requests = within(server).await.unwrap();
    assert_eq!(requests.len(), 5);
    assert!(requests[0].starts_with("GET /user?"));
    assert!(requests[1].starts_with("GET /games?"));
    assert!(requests[2].starts_with("POST /cgi "));
    assert!(requests[3].starts_with("POST /cgi2 "));
    assert!(requests[4].starts_with("GET /sgf?"));
    for request in &requests[2..4] {
        assert!(request.ends_with("chessid=1790936190030046556"));
        assert!(request.contains("okhttp/3.12.12"));
    }
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn next_page_failure_keeps_cursor_retryable_without_changing_history() {
    let (client, server) = replay(vec![r#"{"result":9}"#.into(), EMPTY_PAGE.into()]).await;
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = spawn(directory.path(), client).await;
    assert!(matches!(
        within(app.fox_next_page("123".into(), "456".into())).await,
        Err(AppError::Fox(FoxError::Api { code: 9 }))
    ));
    let page = within(app.fox_next_page("123".into(), "456".into()))
        .await
        .unwrap();
    assert!(page.games.is_empty());
    assert!(page.next_cursor.is_none());
    assert!(within(app.settings()).await.unwrap().fox_history.is_empty());
    let requests = within(server).await.unwrap();
    assert_eq!(requests[0], requests[1]);
    assert!(requests[0].contains("lastcode=456"));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn failed_history_save_keeps_previous_in_memory_settings() {
    let (client, server) = replay(vec![EMPTY_PAGE.into()]).await;
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = spawn(directory.path(), client).await;
    std::fs::create_dir(directory.path().join("settings.json")).unwrap();
    assert!(matches!(
        within(app.fox_search(FoxQuery::Uid("123".into()))).await,
        Err(AppError::Settings(_))
    ));
    assert!(within(app.settings()).await.unwrap().fox_history.is_empty());
    within(server).await.unwrap();
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn failed_download_or_malformed_sgf_preserves_current_document_and_history() {
    let (client, server) = replay(vec![
        EMPTY_PAGE.into(),
        r#"{"result":9}"#.into(),
        r#"{"result":0,"chess":"(;C[unterminated"}"#.into(),
    ])
    .await;
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = spawn(directory.path(), client).await;
    within(app.fox_search(FoxQuery::Uid("123".into())))
        .await
        .unwrap();
    let history = within(app.settings()).await.unwrap().fox_history;
    let revision = within(app.open_bytes(b"(;SZ[9]C[keep];B[aa])".to_vec()))
        .await
        .unwrap();
    document_event(&mut events, revision).await;
    let before = directory.path().join("before.sgf");
    let after = directory.path().join("after.sgf");
    within(app.save(&before)).await.unwrap();

    assert!(matches!(
        within(app.fox_open_game("456".into())).await,
        Err(AppError::Fox(FoxError::DownloadFailed { .. }))
    ));
    assert!(matches!(
        within(app.fox_open_game("789".into())).await,
        Err(AppError::Fox(FoxError::DownloadFailed { .. }))
    ));
    assert_eq!(within(app.settings()).await.unwrap().fox_history, history);
    within(app.save(&after)).await.unwrap();
    assert_eq!(
        std::fs::read(before).unwrap(),
        std::fs::read(after).unwrap()
    );
    assert_eq!(within(app.step(1)).await.unwrap(), revision + 1);
    assert_eq!(
        document_event(&mut events, revision + 1)
            .await
            .board
            .unwrap()
            .move_number,
        1
    );
    within(server).await.unwrap();
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn public_fox_kisei_sample_opens_all_313_moves_and_preserves_metadata() {
    // Public SGF quoted in leela-zero/leela-zero#2285. This tests local import
    // of that published game, not a recording of the current Fox HTTP API.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/sgf/fox-public-kisei-2019.sgf");
    let source = sgf::parse(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(source.games.len(), 1);
    let properties = &source.games[0].sequence[0].properties;
    for (name, expected) in [
        ("PB", "山下敬吾"),
        ("PW", "井山裕太"),
        ("DT", "2019-03-15"),
        ("RE", "W+6.50"),
        ("KM", "650"),
        ("RU", "Japanese"),
    ] {
        assert_eq!(
            properties
                .iter()
                .find(|property| property.identifier == name)
                .unwrap()
                .values,
            [expected]
        );
    }
    assert!(properties.iter().any(|property| property.identifier == "AP"
        && property.values.iter().any(|value| value == "foxwq")));

    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let revision = within(app.open_path(&path)).await.unwrap();
    let opened = document_event(&mut events, revision).await;
    assert_eq!(opened.tree.main_line.len(), 314);
    assert_eq!(opened.tree.current, opened.tree.root);
    assert_eq!(opened.board.unwrap().move_number, 0);
    assert!(opened.position_error.is_none());
    let final_revision = within(app.step(i32::MAX)).await.unwrap();
    let final_view = document_event(&mut events, final_revision).await;
    assert!(final_view.position_error.is_none());
    assert_eq!(final_view.board.unwrap().move_number, 313);
    assert_eq!(
        final_view.tree.current,
        *opened.tree.main_line.last().unwrap()
    );

    let saved = directory.path().join("kisei.sgf");
    within(app.save(&saved)).await.unwrap();
    let exported = sgf::parse(&std::fs::read(saved).unwrap()).unwrap();
    // Preserve the original names, result, duplicate AP entries, raw KM and
    // every node/comment. Export only performs SGF's documented UTF-8 handling.
    assert_eq!(exported.games, source.games);
    assert!(within(app.settings()).await.unwrap().fox_history.is_empty());
    within(app.shutdown()).await.unwrap();
}
