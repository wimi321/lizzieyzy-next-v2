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
fn instant_response_is_opt_in_and_cannot_be_repeated() {
    assert!(!parse(&[]).instant_response);
    assert!(parse(&["--instant-response"]).instant_response);
    assert!(
        Options::parse(["analysis", "--instant-response", "--instant-response"].map(str::to_owned))
            .is_err()
    );
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

#[test]
fn human_model_is_explicit_and_cannot_be_repeated() {
    assert!(!parse(&[]).human_model);
    assert!(parse(&["-human-model", "unused-human.bin.gz"]).human_model);
    assert!(
        Options::parse(
            ["analysis", "-human-model", "one", "-human-model", "two"].map(str::to_owned)
        )
        .is_err()
    );
}

#[test]
fn human_model_failure_is_opt_in_and_cannot_be_repeated() {
    assert!(!parse(&[]).fail_human_model);
    assert!(parse(&["--fail-human-model"]).fail_human_model);
    assert!(
        Options::parse(["analysis", "--fail-human-model", "--fail-human-model"].map(str::to_owned))
            .is_err()
    );
}

#[test]
fn empty_override_removes_the_key_and_restores_default_threads() {
    assert_eq!(
        parse(&[
            "-override-config",
            "numAnalysisThreads=4,numAnalysisThreads="
        ])
        .threads,
        1
    );
    assert_eq!(
        parse(&[
            "-override-config",
            "numAnalysisThreads=,numAnalysisThreads=2"
        ])
        .threads,
        2
    );
    assert_eq!(
        parse(&[
            "--threads",
            "3",
            "-override-config",
            "numAnalysisThreads=4,numAnalysisThreads="
        ])
        .threads,
        3
    );
    assert!(
        parse(&[
            "-override-config",
            "unknown=,reportAnalysisWinratesAs=WHITE,reportAnalysisWinratesAs="
        ])
        .report_perspective
        .is_none()
    );
}

#[test]
fn startup_options_validate_backend_and_bounds() {
    let options = parse(&[
        "--backend",
        "opencl",
        "--tuning-lines",
        "4",
        "--tuning-interval-ms",
        "10",
        "--startup-error",
        "GPU memory exhausted",
        "--startup-script-relative",
        "../engine/startup.json",
    ]);
    assert_eq!(options.backend.as_deref(), Some("opencl"));
    assert_eq!(options.tuning_lines, 4);
    assert_eq!(options.tuning_interval_ms, 10);
    assert_eq!(
        options.startup_error.as_deref(),
        Some("GPU memory exhausted")
    );
    for args in [
        ["analysis", "--backend", "unknown"],
        ["analysis", "--tuning-lines", "1001"],
        ["analysis", "--tuning-interval-ms", "86400001"],
    ] {
        assert!(Options::parse(args.map(str::to_owned)).is_err());
    }
}

#[test]
fn raw_output_options_validate_query_numbers_and_paired_line_size() {
    let options = parse(&[
        "--startup-gbk-path",
        "--stdout-line-before-query",
        "2",
        "--stdout-line-bytes",
        "8388608",
        "--stdout-invalid-before-query",
        "3",
        "--stderr-invalid-flood-bytes",
        "1048576",
    ]);
    assert!(options.startup_gbk_path);
    assert_eq!(options.stdout_line_before_query, Some(2));
    assert_eq!(options.stdout_line_bytes, Some(8388608));
    assert_eq!(options.stdout_invalid_before_query, Some(3));
    assert_eq!(options.stderr_invalid_flood_bytes, Some(1048576));
    for args in [
        vec![
            "--stdout-line-before-query",
            "0",
            "--stdout-line-bytes",
            "32",
        ],
        vec![
            "--stdout-line-before-query",
            "1",
            "--stdout-line-bytes",
            "31",
        ],
        vec!["--stdout-line-before-query", "1"],
        vec!["--stdout-line-bytes", "32"],
        vec!["--stdout-invalid-before-query", "0"],
        vec!["--stderr-invalid-flood-bytes", "-1"],
    ] {
        assert!(
            Options::parse(std::iter::once("analysis").chain(args).map(str::to_owned)).is_err()
        );
    }
}
