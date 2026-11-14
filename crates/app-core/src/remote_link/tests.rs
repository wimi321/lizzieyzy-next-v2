#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use proptest::prelude::*;

proptest! {
    #[test]
    fn normalizing_an_extracted_link_is_idempotent(
        secure in any::<bool>(),
        port in 1_u16..=u16::MAX,
        path in "[a-zA-Z0-9_~-]{0,64}",
        token in "[a-zA-Z0-9_~-]{0,64}",
    ) {
        let scheme = if secure { "wss" } else { "ws" };
        let expected = format!("{scheme}://127.0.0.1:{port}/{path}?token={token}");
        let extracted = normalize_remote_link(&format!("链接：\"{expected}\" 后续文字"))?;
        prop_assert_eq!(&extracted, &expected);
        prop_assert_eq!(normalize_remote_link(&extracted)?, expected);
    }
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/remote")
        .join(name)
}

#[test]
fn normalizes_width_quotes_and_embedded_links_without_rewriting_tokens() {
    for (input, expected) in [
        (
            "  'ws://127.0.0.1:1234/analysis'  ",
            "ws://127.0.0.1:1234/analysis",
        ),
        (
            "\u{3000}＂ｗｓｓ：／／localhost／a？token＝a％２Ｆb＂\u{3000}",
            "wss://localhost/a?token=a%2Fb",
        ),
        (
            "粘贴链接：<wss://localhost/analysis?token=opaque> 后续文字",
            "wss://localhost/analysis?token=opaque",
        ),
        ("地址：WS://LOCALHOST/a\nother text", "WS://LOCALHOST/a"),
        ("wss://localhost/a\" ignored", "wss://localhost/a"),
        ("ws://[::1]:1234/analysis", "ws://[::1]:1234/analysis"),
        (
            "ws://localhost/first wss://localhost/second",
            "ws://localhost/first",
        ),
    ] {
        assert_eq!(normalize_remote_link(input), Ok(expected.into()));
    }
}

#[test]
fn rejects_missing_host_wrong_scheme_and_ambiguous_authorities() {
    for input in [
        "",
        "   ",
        "'",
        "https://localhost/a",
        "完善://localhost/a",
        "ws://",
        "wss:///path",
        "ws://?token=opaque",
        "ws://#fragment",
        "ws://:1234/a",
        "ws://localhost:invalid/a",
        "ws://localhost:65536/a",
        "ws://[broken/a",
        "ws://localhost\\path",
        "ws://localhost/a\0secret",
    ] {
        assert_eq!(
            normalize_remote_link(input),
            Err(RemoteLinkError::InvalidLink)
        );
    }
}

#[tokio::test]
async fn recognizes_png_and_jpeg_qr_then_normalizes_the_link() {
    for name in ["link.png", "link.jpg"] {
        assert_eq!(
            read_remote_link_qr(fixture(name)).await,
            Ok("ws://127.0.0.1:12345/analysis?token=fixture-only".into())
        );
    }
}

#[tokio::test]
async fn rejects_non_image_missing_qr_and_non_remote_qr() {
    let directory = tempfile::tempdir().expect("temporary directory");
    assert_eq!(
        read_remote_link_qr(directory.path().to_owned()).await,
        Err(RemoteLinkError::InvalidImage)
    );
    assert_eq!(
        read_remote_link_qr(fixture("not-image.txt")).await,
        Err(RemoteLinkError::InvalidImage)
    );
    assert_eq!(
        read_remote_link_qr(fixture("blank.png")).await,
        Err(RemoteLinkError::NoQrCode)
    );
    assert_eq!(
        read_remote_link_qr(fixture("not-remote.png")).await,
        Err(RemoteLinkError::InvalidLink)
    );
    assert_eq!(
        read_remote_link_qr(fixture("missing.png")).await,
        Err(RemoteLinkError::ImageRead)
    );
}

#[tokio::test]
async fn rejects_oversized_file_without_decoding_it() {
    let file = tempfile::NamedTempFile::new().expect("temporary image");
    file.as_file()
        .set_len(MAX_IMAGE_BYTES + 1)
        .expect("sparse image length");
    assert_eq!(
        read_remote_link_qr(file.path().to_owned()).await,
        Err(RemoteLinkError::ImageTooLarge)
    );
}

#[test]
fn link_failures_do_not_include_the_link_or_token() {
    let error =
        normalize_remote_link("https://localhost?token=do-not-log-this").expect_err("wrong scheme");
    let output = format!(
        "{error} {error:?} {}",
        serde_json::to_string(&error).expect("serialize")
    );
    assert!(!output.contains("do-not-log-this"));
    assert!(!output.contains("localhost"));
}
