#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "zhizi_account/support.rs"]
mod support;
use app_core::{commentary_keys::SecretStore, zhizi::*};
use common::within;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use support::{Server, Store, Stub, body};
const LOGIN: &str = "/api/cluster/account/login";
const FAST: &str = "/api/cluster/account/fast-login";
const RESET: &str = "/api/cluster/account/reset-password";
const CODE: &str = "/api/cluster/account/send-code";
const SOCKET: &str = "/api/cluster/account/fetch-socketio-token";
const ACCOUNT: &str = "/api/cluster/account/me";
const BALANCE: &str = "/api/cluster/balance";
const RECENT: &str = "/api/cluster/usage/my-usages?page=0&pageSize=5";

fn service(server: &Server, store: Option<Arc<Store>>) -> ZhiziService {
    ZhiziService::spawn(
        Some(ZhiziClient::with_base_url(&server.base).unwrap()),
        store.map(|v| v as Arc<dyn SecretStore>),
    )
}

#[tokio::test]
async fn all_authentication_endpoints_preserve_password_and_describe_code_registration() {
    let mut server = Server::new(vec![
        Stub::json(LOGIN, json!({"token":"password-token"})),
        Stub::json(FAST, json!({"token":"code-token"})),
        Stub::json(CODE, json!({})).header("Retry-After", "91"),
        Stub::json(RESET, json!({"token":"reset-token"})),
    ])
    .await;
    let service = service(&server, None);
    let status = within(service.login_password(
        "  user@example.test  ".into(),
        "  password  ".into(),
        false,
    ))
    .await
    .unwrap();
    assert!(status.logged_in && !status.remembered && !status.may_register_account);
    assert_eq!(
        body(&server.next().await),
        json!({"email":"user@example.test","password":"  password  "})
    );
    let status = within(service.login_code("  13900000000  ".into(), "123456".into(), false))
        .await
        .unwrap();
    assert!(status.may_register_account);
    assert_eq!(
        body(&server.next().await),
        json!({"phone":"13900000000","verificationCode":"123456"})
    );
    let delivery = within(service.send_code(
        "user@example.test".into(),
        ZhiziVerificationPurpose::ResetPassword,
    ))
    .await
    .unwrap();
    assert_eq!(delivery.retry_after_seconds, 91);
    assert!(!delivery.may_register_account);
    assert_eq!(
        body(&server.next().await),
        json!({"email":"user@example.test","type":"reset_password"})
    );
    assert_eq!(
        within(service.reset_password(
            "user@example.test".into(),
            "123456".into(),
            "short".into(),
            false
        ))
        .await
        .unwrap_err()
        .kind,
        ZhiziErrorKind::PasswordTooShort
    );
    let status = within(service.reset_password(
        "user@example.test".into(),
        "123456".into(),
        " password ".into(),
        false,
    ))
    .await
    .unwrap();
    assert!(status.logged_in);
    assert_eq!(
        body(&server.next().await),
        json!({"email":"user@example.test","verificationCode":"123456","password":" password "})
    );
}

#[tokio::test]
async fn remembered_token_restores_switches_and_logout_only_deletes_zhizi_slot() {
    let mut server = Server::new(vec![
        Stub::json(LOGIN, json!({"token":"first-token"})),
        Stub::json(LOGIN, json!({"token":"second-token"})),
        Stub::json(LOGIN, json!({"token":"memory-token"})),
    ])
    .await;
    let store = Store::new();
    within(store.write("remote:unrelated".into(), "remote-token".into()))
        .await
        .unwrap();
    within(store.write("https://commentary.test/v1".into(), "ai-key".into()))
        .await
        .unwrap();
    let service = service(&server, Some(store.clone()));
    assert!(!service.status().logged_in);
    let status =
        within(service.login_password("first@example.test".into(), "password".into(), true))
            .await
            .unwrap();
    assert!(status.logged_in && status.remembered);
    server.next().await;
    assert_eq!(
        store.inspect().await.get("zhizi:login"),
        Some(&"first-token".to_owned())
    );
    let restored = ZhiziService::spawn(None, Some(store.clone()));
    assert!(!restored.status().logged_in);
    assert!(within(restored.restore()).await.unwrap().logged_in);
    within(service.login_password("second@example.test".into(), "password".into(), true))
        .await
        .unwrap();
    server.next().await;
    assert_eq!(
        store.inspect().await.get("zhizi:login"),
        Some(&"second-token".to_owned())
    );
    within(service.login_password("third@example.test".into(), "password".into(), false))
        .await
        .unwrap();
    server.next().await;
    assert!(!store.inspect().await.contains_key("zhizi:login"));
    assert!(!within(service.logout()).await.unwrap().logged_in);
    let entries = store.inspect().await;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries["remote:unrelated"], "remote-token");
    assert_eq!(entries["https://commentary.test/v1"], "ai-key");
    assert!(!within(service.restore()).await.unwrap().logged_in);
}

#[tokio::test]
async fn key_store_failure_is_soft_and_failed_deletion_is_retried_without_restoring_token() {
    let mut server = Server::new(vec![
        Stub::json(LOGIN, json!({"token":"first-token"})),
        Stub::json(LOGIN, json!({"token":"memory-token"})),
    ])
    .await;
    let store = Store::new();
    let service = service(&server, Some(store.clone()));
    within(service.login_password("user@example.test".into(), "password".into(), true))
        .await
        .unwrap();
    server.next().await;
    store.available(false).await;
    let restarted = ZhiziService::spawn(None, Some(store.clone()));
    assert!(!within(restarted.restore()).await.unwrap().logged_in);
    let status =
        within(service.login_password("second@example.test".into(), "password".into(), true))
            .await
            .unwrap();
    server.next().await;
    assert!(status.logged_in && !status.remembered && !status.store_available);
    assert!(!within(service.logout()).await.unwrap().logged_in);
    store.available(true).await;
    assert!(!within(service.restore()).await.unwrap().logged_in);
    assert!(store.inspect().await.is_empty());
}

#[tokio::test]
async fn socket_token_is_fresh_bearer_authenticated_and_rejects_insecure_addresses() {
    let mut server = Server::new(vec![
        Stub::json(LOGIN, json!({"token":"login-token"})),
        Stub::json(
            SOCKET,
            json!({"token":"session-one","socketIOURL":"https://localhost:1234"}),
        ),
        Stub::json(
            SOCKET,
            json!({"token":"session-two","socketIOURL":"wss://localhost:1234"}),
        ),
        Stub::json(
            SOCKET,
            json!({"token":"session-three","socketIOURL":"http://localhost:1234"}),
        ),
    ])
    .await;
    let service = service(&server, None);
    assert_eq!(
        within(service.socket_token("args".into()))
            .await
            .unwrap_err()
            .kind,
        ZhiziErrorKind::LoginRequired
    );
    within(service.login_password("user@example.test".into(), "password".into(), false))
        .await
        .unwrap();
    server.next().await;
    for expected in ["session-one", "session-two"] {
        let token = within(service.socket_token("--platform all --engine-type go".into()))
            .await
            .unwrap();
        assert_eq!(token.token, expected);
        assert!(!format!("{token:?}").contains(expected));
        let request = server.next().await;
        assert!(
            request
                .to_lowercase()
                .contains("authorization: bearer login-token")
        );
        assert_eq!(
            body(&request),
            json!({"args":"--platform all --engine-type go"})
        );
    }
    assert_eq!(
        within(service.socket_token("args".into()))
            .await
            .unwrap_err()
            .kind,
        ZhiziErrorKind::UnsafeAddress
    );
}

#[tokio::test]
async fn unauthorized_response_invalidates_memory_and_stored_token() {
    let mut server = Server::new(vec![
        Stub::json(LOGIN, json!({"token":"saved-token"})),
        Stub::error(SOCKET, 401, "unauthorized").header("x-request-id", "req-401"),
    ])
    .await;
    let store = Store::new();
    let service = service(&server, Some(store.clone()));
    within(service.login_password("user@example.test".into(), "password".into(), true))
        .await
        .unwrap();
    server.next().await;
    let error = within(service.socket_token("args".into()))
        .await
        .unwrap_err();
    assert!(error.is_fatal() && error.is_unauthorized());
    assert_eq!(error.request_id, "req-401");
    assert!(!service.status().logged_in);
    assert!(store.inspect().await.is_empty());
    assert_eq!(
        within(service.socket_token("args".into()))
            .await
            .unwrap_err()
            .kind,
        ZhiziErrorKind::LoginRequired
    );
}

#[tokio::test]
async fn typed_error_keys_request_ids_and_plain_401_fallbacks() {
    use ZhiziErrorKind::*;
    for (key, expected) in [
        ("invalid_phone", InvalidAccount),
        ("invalid_email", InvalidAccount),
        ("invalid_password", InvalidCredentials),
        ("invalid_credentials", InvalidCredentials),
        ("invalid_verification_code", InvalidVerificationCode),
        ("fast_login_too_frequent", TooFrequent),
        ("password_too_short", PasswordTooShort),
        ("duplicate_phone", AccountExists),
        ("duplicate_email", AccountExists),
        ("not_found", AccountNotFound),
        ("unauthorized", LoginExpired),
        ("failed_to_insert_socket_io_token", AllocationUnavailable),
        ("network_error", Network),
        ("invalid_response", IncompleteResponse),
        ("insufficient_balance", InsufficientBalance),
        ("vip_expired", VipExpired),
        ("new_server_code", Unknown("new_server_code".into())),
    ] {
        let server = Server::new(vec![
            Stub::error(LOGIN, 400, key).header("x-correlation-id", "corr-17"),
        ])
        .await;
        let service = service(&server, None);
        let error =
            within(service.login_password("user@example.test".into(), "password".into(), false))
                .await
                .unwrap_err();
        assert_eq!(error.kind, expected);
        assert_eq!(error.request_id, "corr-17");
    }
    for (path, code, expected) in [
        (LOGIN, false, InvalidCredentials),
        (FAST, true, InvalidVerificationCode),
    ] {
        let mut stub = Stub::error(path, 401, "");
        stub.body = "Unauthorized".into();
        let server = Server::new(vec![stub]).await;
        let service = service(&server, None);
        let error = if code {
            within(service.login_code("user@example.test".into(), "123456".into(), false))
                .await
                .unwrap_err()
        } else {
            within(service.login_password("user@example.test".into(), "password".into(), false))
                .await
                .unwrap_err()
        };
        assert_eq!(error.kind, expected);
    }
}

#[tokio::test]
async fn verification_cooldown_uses_default_header_cap_and_virtual_time() {
    let mut server = Server::new(vec![
        Stub::json(CODE, json!({})),
        Stub::json(CODE, json!({})).header("Retry-After", "900"),
        Stub::json(CODE, json!({})).header("Retry-After", "7"),
    ])
    .await;
    let service = service(&server, None);
    let first = within(service.send_code(
        "user@example.test".into(),
        ZhiziVerificationPurpose::FastLogin,
    ))
    .await
    .unwrap();
    server.next().await;
    assert_eq!(first.retry_after_seconds, 60);
    assert!(first.may_register_account);
    let error = within(service.send_code(
        "user@example.test".into(),
        ZhiziVerificationPurpose::ResetPassword,
    ))
    .await
    .unwrap_err();
    assert_eq!(error.kind, ZhiziErrorKind::TooFrequent);
    assert_eq!(error.retry_after_seconds, 60);
    advance(Duration::from_secs(60)).await;
    assert_eq!(
        within(service.send_code(
            "user@example.test".into(),
            ZhiziVerificationPurpose::FastLogin
        ))
        .await
        .unwrap()
        .retry_after_seconds,
        600
    );
    server.next().await;
    advance(Duration::from_secs(599)).await;
    assert_eq!(
        within(service.send_code(
            "user@example.test".into(),
            ZhiziVerificationPurpose::FastLogin
        ))
        .await
        .unwrap_err()
        .retry_after_seconds,
        1
    );
    advance(Duration::from_secs(1)).await;
    assert_eq!(
        within(service.send_code(
            "user@example.test".into(),
            ZhiziVerificationPurpose::FastLogin
        ))
        .await
        .unwrap()
        .retry_after_seconds,
        7
    );
}

fn overview_stubs() -> Vec<Stub> {
    let gate = Arc::new(tokio::sync::Barrier::new(3));
    let mut stubs = vec![
        Stub::json(
            ACCOUNT,
            json!({"email":"user@example.test","isMembership":true,"membershipExpiresAt":"2027-01-01T00:00:00Z","membershipAutoRenew":false}),
        ),
        Stub::json(
            BALANCE,
            json!({"remainingBalance":"12.30","totalCashAmount":30,"totalDuration":321,"currentNumOfNodes":4}),
        ),
        Stub::json(
            RECENT,
            json!({"total":1,"items":[{"startedAt":"2026-01-01T00:00:00Z","duration":123,"totalCost":"0.1234","ready":true,"vip":true,"share":true}]}),
        ),
    ];
    for stub in &mut stubs {
        stub.gate = Some(gate.clone());
    }
    stubs
}

#[tokio::test]
async fn overview_fetches_concurrently_caches_45_seconds_and_details_use_twenty_items() {
    let mut stubs = vec![Stub::json(LOGIN, json!({"token":"token"}))];
    stubs.extend(overview_stubs());
    stubs.extend(overview_stubs());
    stubs.extend(overview_stubs());
    stubs.push(Stub::json(
        "/api/cluster/usage/my-usages?page=2&pageSize=20",
        json!({"total":50,"items":[]}),
    ));
    stubs.push(Stub::json("/api/cluster/credit/my-credits?page=1&pageSize=20",json!({"total":25,"items":[{"creditType":"CASH","amount":"10.00","source":"PAYMENT","productName":"VIP","createdAt":"2026-01-01T00:00:00Z"}]})));
    let mut server = Server::new(stubs).await;
    let service = service(&server, None);
    within(service.login_password("user@example.test".into(), "password".into(), false))
        .await
        .unwrap();
    server.next().await;
    let overview = within(service.overview(false)).await.unwrap();
    for _ in 0..3 {
        assert!(
            server
                .next()
                .await
                .to_lowercase()
                .contains("authorization: bearer token")
        );
    }
    assert!(overview.account.membership);
    assert_eq!(overview.balance.remaining_balance_yuan, "12.30");
    assert_eq!(overview.recent_usage.page_size, 5);
    assert_eq!(overview.recent_usage.items[0].duration_seconds, 123);
    advance(Duration::from_secs(44)).await;
    assert_eq!(within(service.overview(false)).await.unwrap(), overview);
    assert!(server.requests.try_recv().is_err());
    advance(Duration::from_secs(1)).await;
    within(service.overview(false)).await.unwrap();
    for _ in 0..3 {
        server.next().await;
    }
    within(service.overview(true)).await.unwrap();
    for _ in 0..3 {
        server.next().await;
    }
    let page = within(service.usages(2)).await.unwrap();
    assert_eq!((page.page, page.page_size, page.total), (2, 20, 50));
    let page = within(service.credits(1)).await.unwrap();
    assert_eq!((page.page, page.page_size, page.total), (1, 20, 25));
    assert_eq!(page.items[0].amount_yuan, "10.00");
}

#[tokio::test]
async fn incomplete_responses_network_failures_and_unsafe_http_are_typed() {
    for body in [
        json!({}),
        json!({"token":""}),
        json!([]),
        json!({"token":"bad\nvalue"}),
    ] {
        let server = Server::new(vec![Stub::json(LOGIN, body)]).await;
        let service = service(&server, None);
        assert_eq!(
            within(service.login_password("user@example.test".into(), "password".into(), false))
                .await
                .unwrap_err()
                .kind,
            ZhiziErrorKind::IncompleteResponse
        );
    }
    for url in [
        "http://zhizigo.com",
        "http://localhost.example",
        "ftp://127.0.0.1",
        "https://user:pass@example.test",
    ] {
        assert_eq!(
            ZhiziClient::with_base_url(url).unwrap_err().kind,
            ZhiziErrorKind::UnsafeAddress
        );
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let service = ZhiziService::spawn(Some(ZhiziClient::with_base_url(&url).unwrap()), None);
    assert_eq!(
        within(service.login_password("user@example.test".into(), "password".into(), false))
            .await
            .unwrap_err()
            .kind,
        ZhiziErrorKind::Network
    );
    assert_eq!(website_url(), "https://www.zhizigo.com/");
}

#[tokio::test]
#[ignore = "manual Zhizi online acceptance; reads explicit environment credentials"]
async fn zhizi_live_login_and_account_overview() {
    let account = std::env::var("ZHIZI_ACCOUNT").expect("ZHIZI_ACCOUNT required");
    let password = std::env::var("ZHIZI_PASSWORD").expect("ZHIZI_PASSWORD required");
    let service = ZhiziService::spawn(None, None);
    service
        .login_password(account, password, false)
        .await
        .unwrap();
    service.overview(true).await.unwrap();
    service.logout().await.unwrap();
}

#[tokio::test]
async fn settings_and_diagnostic_files_contain_no_account_password_or_token() {
    use tracing_subscriber::layer::SubscriberExt;
    let directory = tempfile::tempdir().unwrap();
    let support = app_core::support::spawn(directory.path().to_owned())
        .await
        .unwrap();
    within(support.start_full_log()).await.unwrap();
    let mut server = Server::new(vec![
        Stub::json(LOGIN, json!({"token":"never-log-login-token"})),
        Stub::error(SOCKET, 400, "unknown-never-log-login-token")
            .header("x-request-id", "private-person@example.test"),
    ])
    .await;
    let subscriber = tracing_subscriber::registry().with(support.layer());
    let service = tracing::subscriber::with_default(subscriber, || service(&server, None));
    within(service.login_password(
        "private-person@example.test".into(),
        "never-log-password".into(),
        false,
    ))
    .await
    .unwrap();
    server.next().await;
    let error = within(service.socket_token("args".into()))
        .await
        .unwrap_err();
    server.next().await;
    assert_eq!(error.request_id, "redacted");
    assert_eq!(error.kind, ZhiziErrorKind::Unknown("redacted".into()));
    let subscriber = tracing_subscriber::registry().with(support.layer());
    tracing::subscriber::with_default(
        subscriber,
        || tracing::info!(target: "zhizi_test", ?error, ?service, "Zhizi diagnostic capture"),
    );
    let settings = app_core::Settings::default();
    within(settings.save(directory.path())).await.unwrap();
    within(support.flush()).await.unwrap();
    let settings_text = std::fs::read_to_string(directory.path().join("settings.json")).unwrap();
    let logs = std::fs::read_to_string(directory.path().join("logs/app.log")).unwrap();
    assert!(logs.contains("Zhizi diagnostic capture"), "{logs}");
    for secret in [
        "private-person@example.test",
        "never-log-password",
        "never-log-login-token",
    ] {
        assert!(!settings_text.contains(secret));
        assert!(!logs.contains(secret));
        assert!(!serde_json::to_string(&error).unwrap().contains(secret));
    }
    within(support.shutdown()).await.unwrap();
}

async fn advance(duration: Duration) {
    tokio::time::pause();
    tokio::time::advance(duration).await;
    tokio::time::resume();
}

#[tokio::test]
async fn key_store_operations_time_out_without_preventing_memory_login_or_startup() {
    for phase in ["read", "write", "delete"] {
        let server = Server::new(vec![Stub::json(LOGIN, json!({"token":"temporary-token"}))]).await;
        let (operations, mut observed) = tokio::sync::mpsc::unbounded_channel();
        let store = Arc::new(support::HangingStore { phase, operations });
        let service = ZhiziService::spawn(
            Some(ZhiziClient::with_base_url(&server.base).unwrap()),
            Some(store),
        );
        let request = {
            let service = service.clone();
            tokio::spawn(async move {
                if phase == "read" {
                    service.restore().await
                } else {
                    service
                        .login_password("user@example.test".into(), "password".into(), true)
                        .await
                }
            })
        };
        within(async { while observed.recv().await.unwrap() != phase {} }).await;
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(2)).await;
        let status = within(request).await.unwrap().unwrap();
        tokio::time::resume();
        assert_eq!(status.logged_in, phase != "read");
        assert!(!status.remembered && !status.store_available);
    }
}

#[tokio::test]
async fn incomplete_socket_response_does_not_echo_new_session_token_in_error() {
    for value in [
        json!({"token":"socket-secret"}),
        json!({"token":"socket-secret","socketIOURL":"http://127.0.0.1:1234"}),
    ] {
        let mut server = Server::new(vec![
            Stub::json(LOGIN, json!({"token":"account-token"})),
            Stub::json(SOCKET, value).header("x-request-id", "socket-secret"),
        ])
        .await;
        let service = service(&server, None);
        within(service.login_password("user@example.test".into(), "password".into(), false))
            .await
            .unwrap();
        server.next().await;
        let error = within(service.socket_token("args".into()))
            .await
            .unwrap_err();
        assert_eq!(error.request_id, "redacted");
        assert!(
            !serde_json::to_string(&error)
                .unwrap()
                .contains("socket-secret")
        );
    }
}

#[tokio::test]
async fn timed_out_native_write_finishes_before_queued_delete_without_reviving_login() {
    let mut server = Server::new(vec![Stub::json(
        LOGIN,
        json!({"token":"late-native-token"}),
    )])
    .await;
    let inner = Store::new();
    let (release, gate) = tokio::sync::watch::channel(false);
    let (operations, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let store = Arc::new(support::DelayedNativeStore {
        inner: inner.clone(),
        gate,
        operations,
    });
    let service = ZhiziService::spawn(
        Some(ZhiziClient::with_base_url(&server.base).unwrap()),
        Some(store),
    );
    let login = {
        let service = service.clone();
        tokio::spawn(async move {
            service
                .login_password("user@example.test".into(), "password".into(), true)
                .await
        })
    };
    assert_eq!(within(observed.recv()).await, Some("delete"));
    assert_eq!(within(observed.recv()).await, Some("write-start"));
    server.next().await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(2)).await;
    let status = within(login).await.unwrap().unwrap();
    tokio::time::resume();
    assert!(status.logged_in && !status.remembered && !status.store_available);
    assert!(
        observed.try_recv().is_err(),
        "delete must wait for the uncancellable native write"
    );
    release.send(true).unwrap();
    assert_eq!(within(observed.recv()).await, Some("write-done"));
    assert_eq!(within(observed.recv()).await, Some("delete"));
    // An explicit operation is the cleanup barrier after the native call ends.
    assert!(!within(service.logout()).await.unwrap().logged_in);
    assert!(inner.inspect().await.is_empty());
}

#[tokio::test]
async fn app_core_login_and_settings_save_never_persist_account_password_or_token() {
    let directory = tempfile::tempdir().unwrap();
    let mut server = Server::new(vec![
        Stub::json(LOGIN, json!({"token":"memory-login-secret"})),
        Stub::json(LOGIN, json!({"token":"remembered-login-secret"})),
    ])
    .await;
    let store = Store::new();
    let (app, _events) = within(app_core::spawn_with_config(
        directory.path(),
        app_core::AppConfig {
            zhizi_client: Some(ZhiziClient::with_base_url(&server.base).unwrap()),
            commentary_key_store: Some(store.clone()),
            ..app_core::AppConfig::default()
        },
    ))
    .await
    .unwrap();
    assert!(within(app.settings()).await.unwrap().engine.is_none());
    for (account, password, token, remember) in [
        (
            "private-app-user@example.test",
            " memory-only-password ",
            "memory-login-secret",
            false,
        ),
        (
            "13912345678",
            " remembered-password ",
            "remembered-login-secret",
            true,
        ),
    ] {
        let status = within(app.zhizi_login_password(account.into(), password.into(), remember))
            .await
            .unwrap();
        assert!(status.logged_in);
        assert_eq!(status.remembered, remember);
        let sent = body(&server.next().await);
        assert_eq!(sent["password"], password);
        assert_eq!(
            sent[if account.contains('@') {
                "email"
            } else {
                "phone"
            }],
            account
        );
        // Persist through the real app actor after credentials were accepted.
        within(app.set_play_sound(remember)).await.unwrap();
        let saved = std::fs::read_to_string(directory.path().join("settings.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(json["play_sound"], remember);
        for secret in [
            "private-app-user@example.test",
            "13912345678",
            " memory-only-password ",
            " remembered-password ",
            "memory-login-secret",
            "remembered-login-secret",
        ] {
            assert!(!saved.contains(secret));
        }
        let stored = store.inspect().await;
        assert_eq!(
            stored.get("zhizi:login").map(String::as_str),
            remember.then_some(token)
        );
    }
    within(app.zhizi_logout()).await.unwrap();
    assert!(store.inspect().await.is_empty());
    within(app.shutdown()).await.unwrap();
}
