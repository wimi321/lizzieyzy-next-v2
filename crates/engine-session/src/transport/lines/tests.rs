#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::transport::Output;
use std::{future::ready, time::Duration};
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn line_boundaries_preserve_empty_crlf_and_unterminated_eof() {
    let mut lines = ByteLines::new(&b"\n12345678\r\n123456789\nok\r\nlast\r"[..], 8);
    for expected in ["", "12345678"] {
        let line = lines.next_line().await.unwrap().unwrap();
        assert!(matches!(line.stdout(), Output::Stdout(text) if text == expected));
    }
    assert!(matches!(
        lines.next_line().await.unwrap().unwrap().stdout(),
        Output::MalformedStdout(_)
    ));
    for expected in ["ok", "last\r"] {
        assert!(matches!(
            lines.next_line().await.unwrap().unwrap().stdout(),
            Output::Stdout(text) if text == expected
        ));
    }
    assert!(lines.next_line().await.unwrap().is_none());
}

#[tokio::test]
async fn cancellation_keeps_partial_bytes_and_overflow_until_the_delimiter() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (mut writer, input) = tokio::io::duplex(64);
        let mut lines = ByteLines::new(input, 8);
        for chunk in [b"1234".as_slice(), b"5678", b"discarded", b"more discarded"] {
            writer.write_all(chunk).await.unwrap();
            tokio::select! {
                biased;
                _ = lines.next_line() => panic!("no delimiter or EOF yet"),
                _ = ready(()) => {}
            }
            assert!(lines.prefix.len() <= 8);
            assert!(lines.prefix.capacity() <= 8);
            assert!(lines.length <= 10);
        }
        assert_eq!(lines.prefix, b"12345678");
        writer.write_all(b"\r\nok\n").await.unwrap();
        assert!(matches!(
            lines.next_line().await.unwrap().unwrap().stdout(),
            Output::MalformedStdout(_)
        ));
        assert!(matches!(
            lines.next_line().await.unwrap().unwrap().stdout(),
            Output::Stdout(text) if text == "ok"
        ));
        // Cancel again with a non-overflowing partial line and split CRLF.
        writer.write_all(b"12345678\r").await.unwrap();
        tokio::select! {
            biased;
            _ = lines.next_line() => panic!("the trailing CR is not a delimiter"),
            _ = ready(()) => {}
        }
        writer.write_all(b"\n").await.unwrap();
        assert!(matches!(
            lines.next_line().await.unwrap().unwrap().stdout(),
            Output::Stdout(text) if text == "12345678"
        ));
        drop(writer);
        assert!(lines.next_line().await.unwrap().is_none());
    })
    .await
    .expect("cancellation test safety deadline");
}

#[tokio::test]
async fn truncated_eof_is_reported_once_and_never_retains_the_suffix() {
    let bytes = b"123456789012345\r";
    let mut lines = ByteLines::new(&bytes[..], 8);
    let line = lines.next_line().await.unwrap().unwrap();
    assert!(line.truncated);
    assert_eq!(line.bytes, b"12345678");
    assert!(matches!(line.stdout(), Output::MalformedStdout(_)));
    assert!(lines.next_line().await.unwrap().is_none());
}

#[tokio::test]
async fn uneven_chunks_do_not_grow_allocation_past_the_line_limit() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (mut writer, input) = tokio::io::duplex(8192);
        let mut lines = ByteLines::new(input, STDERR_LIMIT);
        for count in [STDERR_LIMIT - 1, 1, 100] {
            writer.write_all(&vec![b'x'; count]).await.unwrap();
            tokio::select! {
                biased;
                _ = lines.next_line() => panic!("no delimiter yet"),
                _ = ready(()) => {}
            }
            assert!(lines.prefix.capacity() <= STDERR_LIMIT);
        }
        writer.write_all(b"\n").await.unwrap();
        assert_eq!(
            lines.next_line().await.unwrap().unwrap().stderr(),
            "x".repeat(STDERR_LIMIT)
        );
    })
    .await
    .expect("bounded allocation test safety deadline");
}

#[tokio::test]
async fn invalid_utf8_inside_json_is_not_repaired_into_a_valid_response() {
    let mut lines = ByteLines::new(&b"{\"warning\":\"\xFF\"}\n{\"warning\":\"ok\"}\n"[..], 64);
    assert!(matches!(
        lines.next_line().await.unwrap().unwrap().stdout(),
        Output::MalformedStdout("stdout line is not UTF-8")
    ));
    assert!(matches!(
        lines.next_line().await.unwrap().unwrap().stdout(),
        Output::Stdout(text) if text == r#"{"warning":"ok"}"#
    ));
}

#[tokio::test]
async fn stderr_retains_utf8_and_trims_only_an_incomplete_truncated_character() {
    let full = "显".repeat(2000);
    let bytes = format!("UTF-8 中文\r\n{full}\nend\n");
    let mut lines = ByteLines::new(bytes.as_bytes(), STDERR_LIMIT);
    assert_eq!(
        lines.next_line().await.unwrap().unwrap().stderr(),
        "UTF-8 中文"
    );
    let truncated = lines.next_line().await.unwrap().unwrap();
    assert_eq!(truncated.bytes.len(), STDERR_LIMIT);
    assert_eq!(truncated.stderr(), "显".repeat(1365));
    assert_eq!(lines.next_line().await.unwrap().unwrap().stderr(), "end");
}

#[cfg(not(windows))]
#[tokio::test]
async fn stderr_invalid_bytes_are_lossy_and_decoded_expansion_stays_bounded() {
    let bytes = vec![0xff; STDERR_LIMIT + 100];
    let mut lines = ByteLines::new(bytes.as_slice(), STDERR_LIMIT);
    let line = lines.next_line().await.unwrap().unwrap().stderr();
    assert_eq!(line, "�".repeat(1365));
}

#[tokio::test]
async fn genuine_read_errors_are_still_io_errors() {
    struct BrokenPipe;
    impl AsyncRead for BrokenPipe {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<io::Result<()>> {
            std::task::Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)))
        }
    }
    let error = ByteLines::new(BrokenPipe, 8)
        .next_line()
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
}
