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
fn parses_equals_options_with_spaces_windows_paths_and_embedded_equals() {
    for (directory, document) in [
        ("portable settings", "棋谱 file.sgf"),
        (
            r"C:\Users\runner\AppData\Local\Temp\lizzie e2e",
            r"D:\a\棋谱 files\mainline.sgf",
        ),
        ("settings=one", "game=two.sgf"),
    ] {
        let config = format!("--config-dir={directory}");
        let open = format!("--open={document}");
        let expected = StartupOptions {
            config_dir: Some(directory.into()),
            document: Some(document.into()),
        };
        assert_eq!(parse(&[&config, &open]).unwrap(), expected);
        assert_eq!(parse(&[&open, &config]).unwrap(), expected);
    }
}

#[test]
fn allows_mixing_equals_options_and_legacy_arguments() {
    let expected = StartupOptions {
        config_dir: Some("portable settings".into()),
        document: Some("棋谱 file.sgf".into()),
    };
    assert_eq!(
        parse(&["--config-dir=portable settings", "棋谱 file.sgf"]).unwrap(),
        expected
    );
    assert_eq!(
        parse(&["--open=棋谱 file.sgf", "--config-dir", "portable settings"]).unwrap(),
        expected
    );
}

#[test]
fn rejects_duplicates_and_conflicts_across_argument_forms() {
    for args in [
        vec!["--config-dir=a", "--config-dir=b"],
        vec!["--config-dir=a", "--config-dir", "b"],
        vec!["--config-dir", "a", "--config-dir=b"],
        vec!["--config-dir=a", "--config-dir=a"],
        vec!["--config-dir=a", "--config-dir"],
    ] {
        assert!(
            matches!(parse(&args), Err(StartupError::DuplicateConfigDirectory)),
            "{args:?}"
        );
    }
    for args in [
        vec!["--open=a.sgf", "--open=b.sgf"],
        vec!["--open=a.sgf", "b.sgf"],
        vec!["a.sgf", "--open=b.sgf"],
        vec!["--open=a.sgf", "--open=a.sgf"],
    ] {
        assert!(
            matches!(parse(&args), Err(StartupError::MultipleDocuments)),
            "{args:?}"
        );
    }
}

#[test]
fn rejects_empty_equals_values_and_unknown_switches() {
    assert!(matches!(
        parse(&["--config-dir="]),
        Err(StartupError::MissingConfigDirectory)
    ));
    assert!(matches!(
        parse(&["--open="]),
        Err(StartupError::EmptyDocument)
    ));
    for option in [
        "--open",
        "--unknown=value",
        "--open-file=game.sgf",
        "--config-directory=path",
    ] {
        assert!(
            matches!(parse(&[option]), Err(StartupError::UnknownOption(value)) if value == option)
        );
    }
}

#[test]
fn separator_keeps_equals_options_literal_and_equals_values_can_start_with_dash() {
    for document in ["--open=game.sgf", "--config-dir=settings"] {
        assert_eq!(
            parse(&["--", document]).unwrap(),
            StartupOptions {
                config_dir: None,
                document: Some(document.into()),
            }
        );
    }
    assert_eq!(
        parse(&["--config-dir=-settings", "--open=-game.sgf"]).unwrap(),
        StartupOptions {
            config_dir: Some("-settings".into()),
            document: Some("-game.sgf".into()),
        }
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

#[cfg(unix)]
#[test]
fn preserves_non_utf8_native_paths_in_equals_values() {
    use std::os::unix::ffi::OsStringExt;
    let directory = OsString::from_vec(vec![b'c', 0xff, b' ', b'=']);
    let document = OsString::from_vec(vec![b'd', 0xfe, b' ', b'=']);
    let mut config_arg = OsString::from("--config-dir=");
    config_arg.push(&directory);
    let mut open_arg = OsString::from("--open=");
    open_arg.push(&document);
    let options = StartupOptions::parse([config_arg, open_arg]).unwrap();
    assert_eq!(options.config_dir, Some(directory.into()));
    assert_eq!(options.document, Some(document.into()));
}

#[cfg(windows)]
#[test]
fn preserves_unpaired_windows_surrogates_in_equals_values() {
    use std::os::windows::ffi::OsStringExt;
    let directory = OsString::from_wide(&[0x0043, 0x003a, 0x005c, 0xd800, 0x0020]);
    let document = OsString::from_wide(&[0x0044, 0x003a, 0x005c, 0xdc00, 0x003d]);
    let mut config_arg = OsString::from("--config-dir=");
    config_arg.push(&directory);
    let mut open_arg = OsString::from("--open=");
    open_arg.push(&document);
    let options = StartupOptions::parse([config_arg, open_arg]).unwrap();
    assert_eq!(options.config_dir, Some(directory.into()));
    assert_eq!(options.document, Some(document.into()));
}

#[tokio::test]
async fn startup_opens_document_and_uses_only_selected_config_directory() {
    let temp = tempdir().unwrap();
    let selected = temp.path().join("portable settings");
    let fallback = temp.path().join("system");
    let document = temp.path().join("棋谱 file.sgf");
    std::fs::write(&document, "(;SZ[9]C[startup document];B[aa])").unwrap();
    let settings = Settings {
        pondering: false,
        ..Settings::default()
    };
    within(settings.save(&selected)).await.unwrap();
    let mut config_arg = OsString::from("--config-dir=");
    config_arg.push(&selected);
    let mut open_arg = OsString::from("--open=");
    open_arg.push(&document);
    let options = StartupOptions::parse([config_arg, open_arg]).unwrap();
    let (core, mut output) = within(start(options, fallback.clone())).await.unwrap();
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
        Err(StartupError::Core(error)) if matches!(error.as_ref(), AppError::Io(_))
    ));
    std::fs::write(temp.path().join("settings.json"), "invalid JSON").unwrap();
    assert!(matches!(
        within(start(StartupOptions::default(), temp.path().to_owned())).await,
        Err(StartupError::Core(error)) if matches!(error.as_ref(), AppError::Settings(_))
    ));
}
