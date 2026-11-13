use super::{FakeClipboard, Harness, json, local_url};
use crate::tests::request_with_body;
use std::sync::Arc;
use tauri::{
    http::{HeaderMap, HeaderValue},
    ipc::InvokeBody,
};

// One RGBA pixel [12, 34, 56, 78], encoded as a complete static PNG with CRCs.
const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 224, 81, 178, 240, 3, 0, 1,
    89, 0, 181, 70, 203, 42, 7, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

// Independent JavaScript encodeURIComponent wire contract for test input.
fn encode_path(path: &std::path::Path) -> String {
    path.to_str()
        .unwrap()
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn path_headers(value: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(value) = value {
        headers.insert("x-lizzieyzy-path", HeaderValue::from_str(value).unwrap());
    }
    headers
}

async fn harness() -> (Harness, Arc<FakeClipboard>) {
    let clipboard = Arc::new(FakeClipboard::default());
    let harness = Harness::with_config(app_core::AppConfig {
        clipboard: Some(clipboard.clone()),
        ..Default::default()
    })
    .await;
    (harness, clipboard)
}

#[tokio::test]
async fn raw_png_commands_write_exact_bytes_and_copy_decoded_rgba() {
    let (harness, clipboard) = harness().await;
    let path = harness.directory.path().join("棋盘 空格+%2F%23.png");
    let encoded = encode_path(&path);
    for header in [encoded.clone(), encoded.replace("%2B", "+")] {
        assert_eq!(
            request_with_body(
                &harness.main,
                "save_png",
                InvokeBody::Raw(PNG.to_vec()),
                path_headers(Some(&header)),
                local_url(),
            )
            .await
            .unwrap(),
            json!(null)
        );
        assert_eq!(std::fs::read(&path).unwrap(), PNG);
    }
    assert_eq!(
        request_with_body(
            &harness.main,
            "copy_png",
            InvokeBody::Raw(PNG.to_vec()),
            HeaderMap::new(),
            local_url(),
        )
        .await
        .unwrap(),
        json!(null)
    );
    assert_eq!(
        *clipboard.images.lock().unwrap(),
        vec![app_core::ClipboardImage {
            rgba: vec![12, 34, 56, 78],
            width: 1,
            height: 1,
        }]
    );
    assert!(clipboard.text.lock().unwrap().is_none());
    assert_eq!(harness.directory.path().read_dir().unwrap().count(), 2); // resources + PNG
    harness.finish().await;
}

#[tokio::test]
async fn png_commands_reject_json_transport_without_file_or_clipboard_effects() {
    let (harness, clipboard) = harness().await;
    let path = harness.directory.path().join("unchanged.png");
    std::fs::write(&path, b"keep existing file").unwrap();
    let headers = path_headers(Some(&encode_path(&path)));
    for command in ["save_png", "copy_png"] {
        for body in [json!({ "bytes": PNG, "path": path }), json!(PNG), json!({})] {
            let error = request_with_body(
                &harness.main,
                command,
                InvokeBody::Json(body),
                headers.clone(),
                local_url(),
            )
            .await
            .unwrap_err();
            assert_eq!(
                error,
                json!({ "type": "PngRequest", "data": "RawBodyRequired" })
            );
            assert_eq!(std::fs::read(&path).unwrap(), b"keep existing file");
            assert!(clipboard.images.lock().unwrap().is_empty());
        }
    }
    harness.finish().await;
}

#[tokio::test]
async fn raw_png_save_rejects_missing_empty_malformed_and_non_utf8_path_headers() {
    let (harness, clipboard) = harness().await;
    let path = harness.directory.path().join("existing.png");
    std::fs::write(&path, b"keep existing file").unwrap();
    let cases = [
        (None, "MissingPath"),
        (Some(""), "EmptyPath"),
        (Some("%"), "InvalidPathEncoding"),
        (Some("%0"), "InvalidPathEncoding"),
        (Some("%G0"), "InvalidPathEncoding"),
        (Some("%0G"), "InvalidPathEncoding"),
        (Some("%FF"), "InvalidPathEncoding"),
        (Some("%C0%AF"), "InvalidPathEncoding"),
        (Some("%ED%A0%80"), "InvalidPathEncoding"),
    ];
    for (encoded, expected) in cases {
        let encoded = encoded.map(|value| {
            if value.is_empty() {
                String::new()
            } else {
                format!(
                    "{}{value}",
                    encode_path(&harness.directory.path().join("rejected"))
                )
            }
        });
        let error = request_with_body(
            &harness.main,
            "save_png",
            InvokeBody::Raw(PNG.to_vec()),
            path_headers(encoded.as_deref()),
            local_url(),
        )
        .await
        .unwrap_err();
        assert_eq!(error, json!({ "type": "PngRequest", "data": expected }));
    }
    let mut raw_non_utf8 = HeaderMap::new();
    raw_non_utf8.insert(
        "x-lizzieyzy-path",
        HeaderValue::from_bytes(&[0xFF]).unwrap(),
    );
    let mut duplicate = path_headers(Some(&encode_path(&path)));
    duplicate.append(
        "x-lizzieyzy-path",
        HeaderValue::from_str(&encode_path(&harness.directory.path().join("second.png"))).unwrap(),
    );
    for headers in [raw_non_utf8, duplicate] {
        let error = request_with_body(
            &harness.main,
            "save_png",
            InvokeBody::Raw(PNG.to_vec()),
            headers,
            local_url(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            json!({ "type": "PngRequest", "data": "InvalidPathEncoding" })
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), b"keep existing file");
    assert!(clipboard.images.lock().unwrap().is_empty());
    assert_eq!(harness.directory.path().read_dir().unwrap().count(), 2);
    harness.finish().await;
}

#[tokio::test]
async fn raw_png_payload_validation_preserves_typed_errors_and_existing_targets() {
    let (harness, clipboard) = harness().await;
    let path = harness.directory.path().join("existing.png");
    std::fs::write(&path, b"keep existing file").unwrap();
    for command in ["save_png", "copy_png"] {
        for (bytes, expected) in [
            (vec![1, 2, 3], "Invalid"),
            (vec![0; app_core::MAX_PNG_BYTES + 1], "TooLarge"),
        ] {
            let error = request_with_body(
                &harness.main,
                command,
                InvokeBody::Raw(bytes),
                path_headers(Some(&encode_path(&path))),
                local_url(),
            )
            .await
            .unwrap_err();
            assert_eq!(error["type"], "Png");
            assert_eq!(error["data"]["type"], expected);
            assert_eq!(std::fs::read(&path).unwrap(), b"keep existing file");
            assert!(clipboard.images.lock().unwrap().is_empty());
        }
    }
    harness.finish().await;
}

#[tokio::test]
async fn raw_png_commands_keep_the_same_local_main_only_acl() {
    let (harness, clipboard) = harness().await;
    let path = harness.directory.path().join("denied.png");
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "png-other", Default::default())
        .build()
        .unwrap();
    for (window, url) in [
        (&other, local_url()),
        (&harness.main, "https://example.com/"),
    ] {
        for command in ["save_png", "copy_png"] {
            let error = request_with_body(
                window,
                command,
                InvokeBody::Raw(PNG.to_vec()),
                path_headers(Some(&encode_path(&path))),
                url,
            )
            .await
            .unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|text| text.contains("not allowed"))
            );
        }
    }
    assert!(!path.exists());
    assert!(clipboard.images.lock().unwrap().is_empty());
    harness.finish().await;
}
