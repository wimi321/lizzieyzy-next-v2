#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::Options;

fn parse(args: &[&str]) -> Options {
    Options::parse(
        std::iter::once("analysis")
            .chain(args.iter().copied())
            .map(str::to_owned),
    )
    .unwrap()
    .unwrap()
}

#[test]
fn analysis_thread_override_is_used_unless_test_thread_count_is_explicit() {
    assert_eq!(parse(&[]).threads, 1);
    assert_eq!(
        parse(&["-override-config", "numAnalysisThreads=2"]).threads,
        2
    );
    for args in [
        ["--threads", "1", "-override-config", "numAnalysisThreads=2"],
        ["-override-config", "numAnalysisThreads=2", "--threads", "1"],
    ] {
        assert_eq!(parse(&args).threads, 1);
    }
    assert_eq!(
        parse(&[
            "-override-config",
            "numAnalysisThreads=4,numAnalysisThreads=2"
        ])
        .threads,
        2
    );
}

#[test]
fn invalid_analysis_thread_override_is_rejected() {
    for value in ["0", "1025", "2.5", "bad"] {
        assert!(
            Options::parse([
                "analysis".to_owned(),
                "-override-config".to_owned(),
                format!("numAnalysisThreads={value}")
            ])
            .is_err()
        );
    }
}
