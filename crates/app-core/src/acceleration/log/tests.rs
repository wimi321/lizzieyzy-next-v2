#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::{collections::BTreeMap, future::Future};
use tokio::io::AsyncWriteExt;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("log observation must finish")
}

fn transport(args: &[&str]) -> TransportSpec {
    TransportSpec::LocalProcess {
        program: "katago".into(),
        args: args.iter().map(OsString::from).collect(),
        cwd: None,
        env: BTreeMap::new(),
    }
}

#[tokio::test]
async fn logging_replaces_only_log_settings_in_the_single_override() {
    let observation = within(Observation::new()).await.unwrap();
    let mut transport = transport(&[
        "analysis",
        "-model",
        "model.bin.gz",
        "-override-config",
        "gpuToUse=2,logFile=old.log,logDir=old,logDirDated=dated,logToStderr=false,numAnalysisThreads=2,reportAnalysisWinratesAs=BLACK",
    ]);
    observation.apply(&mut transport).unwrap();
    let TransportSpec::LocalProcess { args, .. } = transport else {
        panic!()
    };
    assert_eq!(
        args.iter().filter(|arg| *arg == "-override-config").count(),
        1
    );
    let value = args.last().unwrap().to_str().unwrap();
    for preserved in [
        "gpuToUse=2",
        "numAnalysisThreads=2",
        "reportAnalysisWinratesAs=BLACK",
    ] {
        assert!(value.split(',').any(|assignment| assignment == preserved));
    }
    assert_eq!(value.matches("logFile=").count(), 1);
    assert!(!value.contains("old.log"));
    assert!(value.contains("logDir=,logDirDated=,logToStderr=true"));
    assert_eq!(
        &args[..3],
        ["analysis", "-model", "model.bin.gz"].map(OsString::from)
    );
}

#[tokio::test]
async fn incremental_reads_bound_lines_and_do_not_repeat_or_lose_split_unicode() {
    let mut observation = within(Observation::new()).await.unwrap();
    assert!(within(observation.read()).await.unwrap().is_empty());
    let path = observation.directory.path().join("startup.log");
    let mut file = within(tokio::fs::File::create(path)).await.unwrap();
    within(file.write_all(b"one\r\npar")).await.unwrap();
    assert_eq!(within(observation.read()).await.unwrap(), ["one"]);
    let unicode = "tial 显卡".as_bytes();
    within(file.write_all(&unicode[..7])).await.unwrap();
    assert!(within(observation.read()).await.unwrap().is_empty());
    within(file.write_all(&unicode[7..])).await.unwrap();
    assert!(within(observation.read()).await.unwrap().is_empty());
    within(file.write_all(b"\n")).await.unwrap();
    assert_eq!(within(observation.read()).await.unwrap(), ["partial 显卡"]);
    within(file.write_all(b"OpenCL backend thread 0\n"))
        .await
        .unwrap();
    let many = (0..100).map(|i| format!("line {i}\n")).collect::<String>();
    within(file.write_all(many.as_bytes())).await.unwrap();
    within(observation.read()).await.unwrap();
    assert_eq!(observation.lines().len(), 20);
    assert_eq!(observation.lines()[0], "line 80");
    assert_eq!(observation.lines()[19], "line 99");
    assert_eq!(
        observation.backend_line.as_deref(),
        Some("OpenCL backend thread 0")
    );
    assert!(within(observation.read()).await.unwrap().is_empty());
    within(file.write_all(b"unterminated")).await.unwrap();
    within(observation.read()).await.unwrap();
    observation.finish();
    assert_eq!(observation.lines()[19], "unterminated");
    within(async {
        drop(file);
        drop(observation);
    })
    .await;
}

#[tokio::test]
async fn oversized_log_reads_only_a_bounded_suffix_and_caps_partial_lines() {
    let mut observation = within(Observation::new()).await.unwrap();
    let path = observation.directory.path().join("startup.log");
    within(tokio::fs::write(
        &path,
        format!("{}\nlast\n", "x".repeat(100_000)),
    ))
    .await
    .unwrap();
    let lines = within(observation.read()).await.unwrap();
    assert_eq!(lines, ["last"]);
    within(tokio::fs::write(&path, "z".repeat(10_000)))
        .await
        .unwrap();
    assert!(within(observation.read()).await.unwrap().is_empty());
    observation.finish();
    assert_eq!(observation.lines().last().unwrap().len(), LINE_BYTES);
    within(async {
        drop(observation);
    })
    .await;
}

#[test]
fn override_path_rejects_separators_and_accepts_spaces() {
    for invalid in [
        "/tmp/a,b/log",
        "/tmp/a=b/log",
        "/tmp/a\nb/log",
        "/tmp/a\rb/log",
    ] {
        assert!(validate_path(Path::new(invalid)).is_err());
    }
    assert_eq!(
        validate_path(Path::new("/tmp/a b/log")).unwrap(),
        "/tmp/a b/log"
    );
}
