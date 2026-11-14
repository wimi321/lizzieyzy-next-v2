use super::{Harness, json, local_url, request};
use crate::{error::CommandError, zhizi_website::ZhiziWebsite};
use tauri::Manager;
use tokio::sync::mpsc;

fn opener(harness: &Harness, fail: bool) -> mpsc::UnboundedReceiver<String> {
    let (calls, received) = mpsc::unbounded_channel();
    assert!(harness.app.manage(ZhiziWebsite::with_opener(move |url| {
        calls.send(url.to_owned()).unwrap();
        if fail {
            Err(CommandError::BrowserUnavailable)
        } else {
            Ok(())
        }
    })));
    received
}

#[tokio::test]
async fn open_zhizi_website_uses_fixed_url_without_arguments() {
    let harness = Harness::new().await;
    let mut calls = opener(&harness, false);
    for arguments in [
        json!({}),
        // Unknown IPC keys cannot override the URL or choose a program.
        json!({ "url": "https://example.invalid/", "with": "untrusted-program" }),
    ] {
        assert_eq!(
            harness.invoke("open_zhizi_website", arguments).await,
            json!(null)
        );
        assert_eq!(calls.try_recv().unwrap(), app_core::zhizi::website_url());
        assert!(calls.try_recv().is_err());
    }
    harness.finish().await;
}

#[tokio::test]
async fn open_zhizi_website_returns_typed_browser_failure() {
    let harness = Harness::new().await;
    let mut calls = opener(&harness, true);
    let error = request(&harness.main, "open_zhizi_website", json!({}), local_url())
        .await
        .unwrap_err();
    assert_eq!(error, json!({ "type": "BrowserUnavailable" }));
    assert_eq!(calls.try_recv().unwrap(), app_core::zhizi::website_url());
    assert!(calls.try_recv().is_err());
    harness.finish().await;
}

#[tokio::test]
async fn open_zhizi_website_denies_other_windows_remote_origins_and_generic_opener() {
    let harness = Harness::new().await;
    let mut calls = opener(&harness, false);
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "zhizi-other", Default::default())
        .build()
        .unwrap();
    for (window, url, command, arguments) in [
        (&other, local_url(), "open_zhizi_website", json!({})),
        (
            &harness.main,
            "https://www.zhizigo.com/",
            "open_zhizi_website",
            json!({}),
        ),
        (
            &harness.main,
            local_url(),
            "plugin:opener|open_url",
            json!({ "url": app_core::zhizi::website_url() }),
        ),
    ] {
        let error = request(window, command, arguments, url).await.unwrap_err();
        assert!(
            error
                .as_str()
                .is_some_and(|message| message.contains("not allowed")),
            "{command}: {error}"
        );
        assert!(calls.try_recv().is_err());
    }
    harness.finish().await;
}
