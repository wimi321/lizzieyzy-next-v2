#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use app_core::{AppEvent, Settings};
use std::{future::Future, time::Duration};
use tempfile::tempdir;

fn parse(args: &[&str]) -> Result<StartupOptions, StartupError> {
    StartupOptions::parse(args.iter().map(OsString::from))
}

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("startup operation exceeded safety deadline")
}

#[test]
fn parses_portable_directory_and_one_document_in_either_order() {
    let expected = StartupOptions {
        config_dir: Some("portable settings".into()),
        document: Some("棋谱 file.sgf".into()),
    };
    assert_eq!(
        parse(&["--config-dir", "portable settings", "棋谱 file.sgf"]).unwrap(),
        expected
    );
    assert_eq!(
        parse(&["棋谱 file.sgf", "--config-dir", "portable settings"]).unwrap(),
        expected
    );
    assert_eq!(parse(&[]).unwrap(), StartupOptions::default());
    assert_eq!(
        parse(&["--", "-game.sgf"]).unwrap().document,
        Some("-game.sgf".into())
    );
}

#[test]
fn rejects_ambiguous_or_incomplete_arguments() {
    for args in [
        vec!["--config-dir"],
        vec!["--config-dir", ""],
        vec!["--config-dir", "--"],
    ] {
        assert!(matches!(
            parse(&args),
            Err(StartupError::MissingConfigDirectory)
        ));
    }
    assert!(matches!(
        parse(&["--config-dir", "a", "--config-dir", "b"]),
        Err(StartupError::DuplicateConfigDirectory)
    ));
    assert!(matches!(
        parse(&["--unknown"]),
        Err(StartupError::UnknownOption(_))
    ));
    assert!(matches!(
        parse(&["a.sgf", "b.sgf"]),
        Err(StartupError::MultipleDocuments)
    ));
    assert!(matches!(parse(&[""]), Err(StartupError::EmptyDocument)));
}

#[cfg(unix)]
#[test]
fn preserves_non_utf8_native_paths() {
    use std::os::unix::ffi::OsStringExt;
    let directory = OsString::from_vec(vec![b'c', 0xff]);
    let document = OsString::from_vec(vec![b'd', 0xfe]);
    let options = StartupOptions::parse([
        OsString::from("--config-dir"),
        directory.clone(),
        document.clone(),
    ])
    .unwrap();
    assert_eq!(options.config_dir, Some(directory.into()));
    assert_eq!(options.document, Some(document.into()));
}

#[tokio::test]
async fn startup_opens_document_and_uses_only_selected_config_directory() {
    let temp = tempdir().unwrap();
    let selected = temp.path().join("portable");
    let fallback = temp.path().join("system");
    let document = temp.path().join("棋谱.sgf");
    std::fs::write(&document, "(;SZ[9]C[startup document];B[aa])").unwrap();
    let settings = Settings {
        pondering: false,
        ..Settings::default()
    };
    within(settings.save(&selected)).await.unwrap();
    let (core, mut output) = within(start(
        StartupOptions {
            config_dir: Some(selected.clone()),
            document: Some(document),
        },
        fallback.clone(),
    ))
    .await
    .unwrap();
    assert_eq!(within(core.settings()).await.unwrap(), settings);
    let view = within(async {
        loop {
            if let AppEvent::DocumentChanged { revision, view } = output.recv().await.unwrap() {
                assert_eq!(revision, 1);
                break view;
            }
        }
    })
    .await;
    assert_eq!(view.board.unwrap().width, 9);
    assert_eq!(view.comment.as_deref(), Some("startup document"));
    assert!(selected.is_dir());
    assert!(!fallback.exists());
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn startup_creates_default_config_directory() {
    let temp = tempdir().unwrap();
    let directory = temp.path().join("configuration");
    let (core, _) = within(start(StartupOptions::default(), directory.clone()))
        .await
        .unwrap();
    assert!(directory.is_dir());
    assert_eq!(within(core.settings()).await.unwrap(), Settings::default());
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn startup_reports_config_and_document_failures() {
    let temp = tempdir().unwrap();
    let file = temp.path().join("not-a-directory");
    std::fs::write(&file, "file").unwrap();
    assert!(matches!(
        within(start(StartupOptions::default(), file)).await,
        Err(StartupError::ConfigDirectory { .. })
    ));
    assert!(matches!(
        within(start(
            StartupOptions {
                config_dir: None,
                document: Some(temp.path().join("missing.sgf"))
            },
            temp.path().join("config"),
        ))
        .await,
        Err(StartupError::Core(AppError::Io(_)))
    ));
    std::fs::write(temp.path().join("settings.json"), "invalid JSON").unwrap();
    assert!(matches!(
        within(start(StartupOptions::default(), temp.path().to_owned())).await,
        Err(StartupError::Core(AppError::Settings(_)))
    ));
}
