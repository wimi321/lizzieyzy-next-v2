#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

// Recorded on 2026-10-30: QueryUserInfoPanel, YHWQFetchChessList,
// both TXWQFetchChess CGI endpoints and the H5 YHWQFetchChess endpoint.
// The list is cropped to five games; account names/UIDs are consistently
// anonymized, and credential-like fields are cleared without changing types.

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/fox")
            .join(name),
    )
    .unwrap()
}

struct Replay {
    endpoints: FoxEndpoints,
    task: JoinHandle<Vec<String>>,
}

impl Replay {
    async fn start(steps: Vec<(&str, u16, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let steps = steps
            .into_iter()
            .map(|(request, status, body)| (request.to_owned(), status, body))
            .collect::<Vec<_>>();
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for (expected, status, body) in steps {
                let (mut socket, _) = listener.accept().await.unwrap();
                let request = read_request(&mut socket).await;
                assert!(
                    request.contains(&expected),
                    "expected {expected}, got {request}"
                );
                if request.starts_with("POST") {
                    assert!(
                        request
                            .to_ascii_lowercase()
                            .contains("content-type: application/x-www-form-urlencoded")
                    );
                    assert!(request.contains("okhttp/3.12.12"));
                }
                requests.push(request);
                let response = format!(
                    "HTTP/1.1 {status} Replay\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            requests
        });
        Self {
            endpoints: FoxEndpoints {
                user_url: format!("{base}/user"),
                list_url: format!("{base}/list"),
                sgf_urls: vec![
                    format!("{base}/sgf/1"),
                    format!("{base}/sgf/2"),
                    format!("{base}/sgf/3"),
                ],
            },
            task,
        }
    }

    fn client(&self) -> FoxClient {
        FoxClient::with_endpoints(self.endpoints.clone(), Duration::from_secs(2)).unwrap()
    }

    async fn finish(self) -> Vec<String> {
        tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .unwrap()
            .unwrap()
    }
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut buffer = [0; 4096];
        let count = socket.read(&mut buffer).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(|value| value.parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn recorded_account_and_list_replay() {
    let replay = Replay::start(vec![
        ("GET /user?srcuid=0&username=", 200, fixture("user.json")),
        ("GET /list?srcuid=0&dstuid=", 200, fixture("games.json")),
    ])
    .await;
    let result = replay
        .client()
        .search(&FoxQuery::Nickname("样本棋手甲".into()))
        .await
        .unwrap();
    assert_eq!(result.account.nickname, "样本棋手甲");
    assert_eq!(result.account.uid, "900001");
    assert_eq!(result.page.games.len(), 5);
    assert_eq!(result.page.games[0].black.rank, "7段");
    assert_eq!(result.page.games[0].move_count, 177);
    assert_eq!(result.page.games[2].result.score, Some(0.25));
    replay.finish().await;
}

#[tokio::test]
async fn numeric_nickname_is_resolved_and_uid_search_is_explicit() {
    let replay = Replay::start(vec![
        (
            "GET /user?srcuid=0&username=00123456",
            200,
            json!({"result":0,"uid":"900001","username":"00123456"}).to_string(),
        ),
        (
            "dstuid=900001",
            200,
            r#"{"result":0,"chesslist":[]}"#.into(),
        ),
        (
            "dstuid=00123456",
            200,
            r#"{"result":0,"chesslist":[]}"#.into(),
        ),
    ])
    .await;
    let client = replay.client();
    assert_eq!(
        client
            .search(&FoxQuery::Nickname("00123456".into()))
            .await
            .unwrap()
            .account
            .uid,
        "900001"
    );
    assert_eq!(
        client
            .search(&FoxQuery::Uid("00123456".into()))
            .await
            .unwrap()
            .account
            .uid,
        "00123456"
    );
    for invalid in ["", " 123", "12a34", "１２３", "1&dstuid=2"] {
        assert!(matches!(
            client.search(&FoxQuery::Uid(invalid.into())).await,
            Err(FoxError::InvalidUid)
        ));
    }
    replay.finish().await;
}

#[tokio::test]
async fn account_not_found_is_distinct_from_api_http_and_bad_payloads() {
    let replay = Replay::start(vec![
        (
            "GET /user",
            200,
            r#"{"result":101201,"uid":"0","username":""}"#.into(),
        ),
        (
            "GET /user",
            200,
            r#"{"errcode":5,"errmsg":"server busy"}"#.into(),
        ),
        ("GET /user", 200, r#"{"result":7,"uid":"123"}"#.into()),
        ("GET /user", 503, "unavailable".into()),
        ("GET /user", 200, "not json".into()),
        ("GET /user", 200, r#"{"result":0,"uid":"0"}"#.into()),
        (
            "GET /user",
            200,
            r#"{"result":"broken","errcode":0,"uid":"123"}"#.into(),
        ),
    ])
    .await;
    let client = replay.client();
    assert!(matches!(
        client.lookup("missing").await,
        Err(FoxError::UserNotFound)
    ));
    assert!(matches!(
        client.lookup("busy").await,
        Err(FoxError::Api { code: 5 })
    ));
    assert!(matches!(
        client.lookup("error").await,
        Err(FoxError::Api { code: 7 })
    ));
    assert!(matches!(
        client.lookup("http").await,
        Err(FoxError::Http { status: 503 })
    ));
    assert!(matches!(
        client.lookup("json").await,
        Err(FoxError::Json(_))
    ));
    assert!(matches!(
        client.lookup("no uid").await,
        Err(FoxError::InvalidResponse("uid"))
    ));
    assert!(matches!(
        client.lookup("bad status").await,
        Err(FoxError::InvalidResponse("result"))
    ));
    replay.finish().await;
}

#[tokio::test]
async fn pages_use_last_game_id_and_retry_after_failure() {
    let mut list: Value = serde_json::from_str(&fixture("games.json")).unwrap();
    let row = list["chesslist"][0].clone();
    list["chesslist"] = Value::Array(
        (0..100)
            .map(|index| {
                let mut row = row.clone();
                row["chessid"] = json!(format!("game-{index}"));
                row
            })
            .collect(),
    );
    let replay = Replay::start(vec![
        ("lastcode=0", 200, list.to_string()),
        ("lastcode=game-99", 503, "busy".into()),
        (
            "lastcode=game-99",
            200,
            r#"{"result":0,"chesslist":[]}"#.into(),
        ),
    ])
    .await;
    let client = replay.client();
    let first = client.list("123", None).await.unwrap();
    assert_eq!(first.games.len(), 100);
    assert_eq!(first.next_cursor.as_deref(), Some("game-99"));
    assert!(matches!(
        client.list("123", first.next_cursor.as_deref()).await,
        Err(FoxError::Http { status: 503 })
    ));
    let last = client
        .list("123", first.next_cursor.as_deref())
        .await
        .unwrap();
    assert!(last.games.is_empty());
    assert_eq!(last.next_cursor, None);
    assert!(matches!(
        parse::page(&list, "game-99"),
        Err(FoxError::InvalidResponse("non-advancing cursor"))
    ));
    replay.finish().await;
}

#[test]
fn ranks_and_all_result_kinds_match_java() {
    for (raw, expected) in [
        (0, "18级"),
        (16, "2级"),
        (17, "1级"),
        (18, "1段"),
        (26, "9段"),
        (27, "10段"),
        (100, "P1段"),
        (108, "P9段"),
    ] {
        assert_eq!(format_rank(raw), expected);
    }
    let list: Value = serde_json::from_str(&fixture("games.json")).unwrap();
    for (winner, point, rule, kind, score, unit) in [
        (1, -1, 1, FoxResultKind::Resignation, None, None),
        (2, -2, 0, FoxResultKind::Timeout, None, None),
        (1, -3, 1, FoxResultKind::OtherWin, None, None),
        (
            2,
            375,
            1,
            FoxResultKind::Score,
            Some(3.75),
            Some(FoxScoreUnit::Stones),
        ),
        (
            1,
            50,
            0,
            FoxResultKind::Score,
            Some(0.5),
            Some(FoxScoreUnit::Points),
        ),
        (0, 0, 1, FoxResultKind::Other, None, None),
    ] {
        let mut row = list["chesslist"][0].clone();
        row["winner"] = json!(winner);
        row["point"] = json!(point);
        row["rule"] = json!(rule);
        let parsed = parse::page(&json!({"result":0,"chesslist":[row]}), "0").unwrap();
        let result = &parsed.games[0].result;
        assert_eq!(result.kind, kind);
        assert_eq!(result.score, score);
        assert_eq!(result.unit, unit);
        assert_eq!(
            result.winner,
            match winner {
                1 => Some(FoxColor::Black),
                2 => Some(FoxColor::White),
                _ => None,
            }
        );
        assert_eq!(parsed.next_cursor, None);
    }
}

#[test]
fn list_rejects_missing_content_and_preserves_name_aliases() {
    assert!(matches!(
        parse::page(&json!({"result":0}), "0"),
        Err(FoxError::InvalidResponse("chesslist"))
    ));
    assert!(matches!(
        parse::page(&json!({"errcode":5,"errmsg":"busy"}), "0"),
        Err(FoxError::Api { code: 5 })
    ));
    let list: Value = serde_json::from_str(&fixture("games.json")).unwrap();
    let mut row = list["chesslist"][0].clone();
    for prefix in ["black", "white"] {
        for suffix in ["nick", "nickname", "name", "enname"] {
            row.as_object_mut()
                .unwrap()
                .remove(&format!("{prefix}{suffix}"));
        }
    }
    for suffix in ["enname", "name", "nickname", "nick"] {
        row[format!("black{suffix}")] = json!(format!("black-{suffix}"));
        row[format!("white{suffix}")] = json!(format!("white-{suffix}"));
        let page = parse::page(&json!({"result":0,"chesslist":[row.clone()]}), "0").unwrap();
        assert_eq!(page.games[0].black.nickname, format!("black-{suffix}"));
        assert_eq!(page.games[0].white.nickname, format!("white-{suffix}"));
    }
    row["movenum"] = json!(-1);
    assert!(matches!(
        parse::page(&json!({"result":0,"chesslist":[row]}), "0"),
        Err(FoxError::InvalidResponse("movenum"))
    ));
}

#[tokio::test]
async fn truncated_http_body_is_not_accepted_as_a_complete_payload() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 999\r\nConnection: close\r\n\r\n{\"result\":0}").await.unwrap();
    });
    let client = FoxClient::with_endpoints(
        FoxEndpoints {
            user_url: format!("http://{address}/user"),
            ..FoxEndpoints::default()
        },
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(matches!(
        client.lookup("truncated").await,
        Err(FoxError::Transport(_))
    ));
    task.await.unwrap();
}

#[tokio::test]
async fn download_success_and_ordered_fallbacks() {
    for successful_index in 0..3 {
        let mut steps = Vec::new();
        if successful_index > 0 {
            steps.push(("POST /sgf/1", 503, "busy".into()));
        }
        if successful_index > 1 {
            steps.push(("POST /sgf/2", 200, r#"{"result":0,"chess":""}"#.into()));
        }
        steps.push((
            ["POST /sgf/1", "POST /sgf/2", "GET /sgf/3?chessid=game-1"][successful_index],
            200,
            fixture("sgf.json"),
        ));
        let replay = Replay::start(steps).await;
        let sgf = replay.client().download("game-1").await.unwrap();
        assert!(sgf.contains("PB[样本棋手甲]"));
        assert!(sgf.contains("PW[样本棋手乙]"));
        assert!(sgf.contains("KM[375]"));
        replay.finish().await;
    }
}

#[tokio::test]
async fn recorded_cgi_failures_fall_back_to_h5_in_java_order() {
    let game_id = "1790936190030046556";
    let replay = Replay::start(vec![
        ("POST /sgf/1", 200, fixture("sgf-cgi.json")),
        ("POST /sgf/2", 200, fixture("sgf-cgi2.txt")),
        (
            "GET /sgf/3?chessid=1790936190030046556",
            200,
            fixture("sgf.json"),
        ),
    ])
    .await;
    let sgf = replay.client().download(game_id).await.unwrap();
    assert!(sgf.contains("PB[样本棋手甲]"));
    assert!(sgf.contains("PW[样本棋手乙]"));
    assert!(sgf.contains("KM[375]"));
    assert!(sgf.contains("AP[foxwq]"));
    let requests = replay.finish().await;
    assert_eq!(requests.len(), 3);
    for request in &requests[..2] {
        assert!(request.ends_with(&format!("chessid={game_id}")));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("user-agent: okhttp/3.12.12")
        );
    }
    assert!(!requests[2].contains("okhttp"));
}

#[tokio::test]
async fn download_retains_typed_failures_and_accepts_sgf_whitespace() {
    let replay = Replay::start(vec![
        ("POST /sgf/1", 200, r#"{"result":9}"#.into()),
        ("POST /sgf/2", 200, "{".into()),
        (
            "GET /sgf/3",
            200,
            r#"{"result":0,"chess":"<html>error</html>"}"#.into(),
        ),
    ])
    .await;
    let Err(FoxError::DownloadFailed { failures }) = replay.client().download("game-1").await
    else {
        panic!("expected exhausted endpoints")
    };
    assert!(matches!(failures[0], FoxError::Api { code: 9 }));
    assert!(matches!(failures[1], FoxError::Json(_)));
    assert!(matches!(failures[2], FoxError::InvalidResponse("chess")));
    replay.finish().await;
    let replay = Replay::start(vec![(
        "POST /sgf/1",
        200,
        json!({"result":0,"chess":"(\n;GM[1])"}).to_string(),
    )])
    .await;
    assert_eq!(
        replay.client().download("game-1").await.unwrap(),
        "(;GM[1]FF[4]CA[UTF-8]SZ[19])"
    );
    replay.finish().await;
}

#[tokio::test]
async fn timeout_transport_and_response_limit_are_typed() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        std::future::pending::<()>().await;
    });
    let endpoints = FoxEndpoints {
        user_url: format!("http://{address}/user"),
        ..FoxEndpoints::default()
    };
    let client = FoxClient::with_endpoints(endpoints.clone(), Duration::from_millis(100)).unwrap();
    assert!(matches!(
        client.lookup("timeout").await,
        Err(FoxError::Timeout)
    ));
    task.abort();
    task.await.unwrap_err();
    assert!(matches!(
        client.lookup("refused").await,
        Err(FoxError::Transport(_))
    ));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 9999999\r\n\r\n")
            .await
            .unwrap();
    });
    let client = FoxClient::with_endpoints(
        FoxEndpoints {
            user_url: format!("http://{address}/user"),
            ..endpoints
        },
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(matches!(
        client.lookup("big").await,
        Err(FoxError::ResponseTooLarge)
    ));
    task.await.unwrap();
}

proptest::proptest! {
    #[test]
    fn account_wire_and_dto_round_trip(uid in "[1-9][0-9]{0,18}", nickname in "[^\\p{C}]{1,24}") {
        let raw=json!({"result":0,"uid":uid,"username":nickname});
        let account=parse::account(&raw,"fallback").unwrap();
        let encoded=serde_json::to_string(&account).unwrap();
        let decoded:FoxAccount=serde_json::from_str(&encoded).unwrap();
        proptest::prop_assert_eq!(account,decoded);
    }
}
