#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::{future::Future, time::Duration};

mod process;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("KataGo home test timed out")
}

fn overrides(args: &[OsString]) -> &str {
    assert_eq!(
        args.iter().filter(|arg| *arg == "-override-config").count(),
        1
    );
    let index = args
        .iter()
        .position(|arg| arg == "-override-config")
        .unwrap();
    args[index + 1].to_str().unwrap()
}

#[tokio::test]
async fn home_defaults_merge_once_and_preserve_existing_analysis_overrides() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("data 中文 with spaces");
    let mut args = [
        "analysis",
        "-override-config",
        "reportAnalysisWinratesAs=BLACK,numAnalysisThreads=2,gpuToUse=1",
    ]
    .map(OsString::from)
    .to_vec();
    within(apply(&mut args, &root)).await.unwrap();
    let first = args.clone();
    within(apply(&mut args, &root)).await.unwrap();
    assert_eq!(args, first);
    let entries: Vec<_> = overrides(&args).split(',').collect();
    for value in [
        "reportAnalysisWinratesAs=BLACK",
        "numAnalysisThreads=2",
        "gpuToUse=1",
        "logToStderr=true",
    ] {
        assert!(entries.contains(&value));
    }
    assert!(
        entries.contains(&format!("homeDataDir={}", root.join("katago-home").display()).as_str())
    );
    assert!(root.join("katago-home").is_dir());
    within(tokio::task::spawn_blocking(move || directory.close()))
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn explicit_home_and_stderr_values_including_empty_values_are_not_overridden() {
    let directory = tempfile::tempdir().unwrap();
    for value in [
        "homeDataDir=chosen home,logToStderr=false",
        "homeDataDir=,logToStderr=",
    ] {
        let mut args = ["analysis", "-override-config", value]
            .map(OsString::from)
            .to_vec();
        let original = args.clone();
        within(apply(&mut args, directory.path())).await.unwrap();
        assert_eq!(args, original);
        assert!(!directory.path().join("katago-home").exists());
    }
    within(tokio::task::spawn_blocking(move || directory.close()))
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn unrepresentable_home_path_is_omitted_but_stderr_default_is_kept() {
    let directory = tempfile::tempdir().unwrap();
    for name in ["comma,path", "equals=path", "new\nline", "return\rpath"] {
        let root = directory.path().join(name);
        let mut args = vec![OsString::from("analysis")];
        within(apply(&mut args, &root)).await.unwrap();
        assert_eq!(overrides(&args), "logToStderr=true");
        assert!(!root.exists());
    }
    within(tokio::task::spawn_blocking(move || directory.close()))
        .await
        .unwrap()
        .unwrap();
}
