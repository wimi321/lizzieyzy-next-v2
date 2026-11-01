#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use proptest::prelude::*;

const RECORDING: &[u8] = include_bytes!("../../../../fixtures/katago/benchmark-tune-b11-metal.txt");
const FIXED_RECORDING: &[u8] =
    include_bytes!("../../../../fixtures/katago/benchmark-fixed4-b11-metal-batch.txt");
const ADDITIONAL_REPLAY: &[u8] =
    include_bytes!("../../../../fixtures/katago/benchmark-additional-recommendations.txt");

#[test]
fn metal_b11_recording_uses_official_recommendation_not_fastest_row() {
    let mut parser = BenchmarkParser::default();
    let progress = parser.push(RECORDING).unwrap();
    assert_eq!(progress.len(), 30);
    assert_eq!(progress[0].threads, 5);
    assert_eq!(progress[0].completed_positions, 0);
    assert_eq!(progress[0].total_positions, 3);
    assert_eq!(progress[0].visits_per_second, None);
    assert_eq!(progress[1].visits_per_second, Some(35.83));
    assert_eq!(progress[1].nn_evals_per_second, None);
    assert_eq!(parser.metrics().len(), 6);
    assert_eq!(
        parser.finish().unwrap(),
        BenchmarkMetrics {
            threads: 6,
            completed_positions: 3,
            total_positions: 3,
            visits_per_second: 43.92,
            nn_evals_per_second: 43.25,
            nn_max_batch_size: None,
            num_nn_server_threads_per_model: None,
        }
    );
    let fastest = parser
        .metrics()
        .iter()
        .max_by(|a, b| a.visits_per_second.total_cmp(&b.visits_per_second))
        .unwrap();
    assert_eq!(fastest.threads, 12);
}

#[test]
fn fixed4_metal_recording_preserves_main_measurement_and_applies_batch_advice() {
    let mut parser = BenchmarkParser::with_explicit_threads(4).unwrap();
    let progress = parser.push(FIXED_RECORDING).unwrap();
    assert_eq!(
        progress.len(),
        3,
        "additional comparisons are not thread progress"
    );
    assert_eq!(
        progress
            .iter()
            .map(|p| p.completed_positions)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(
        parser.metrics(),
        &[BenchmarkMetrics {
            threads: 4,
            completed_positions: 2,
            total_positions: 2,
            visits_per_second: 15.75,
            nn_evals_per_second: 15.99,
            nn_max_batch_size: None,
            num_nn_server_threads_per_model: None,
        }]
    );
    let result = parser.finish().unwrap();
    assert_eq!(result.threads, 4);
    assert_eq!(
        (result.visits_per_second, result.nn_evals_per_second),
        (44.95, 44.27)
    );
    assert_eq!(result.nn_max_batch_size, Some(2));
    assert_eq!(result.num_nn_server_threads_per_model, None);
    let mut automatic = BenchmarkParser::default();
    automatic.push(FIXED_RECORDING).unwrap();
    assert_eq!(
        automatic.finish(),
        Err(BenchmarkParseError::MissingRecommendation)
    );
}

#[test]
fn automatic_replay_parses_multiline_server_and_batch_without_overwriting_metrics() {
    let mut parser = BenchmarkParser::default();
    let updates = parser.push(ADDITIONAL_REPLAY).unwrap();
    assert_eq!(updates.len(), 3);
    assert_eq!(parser.metrics().len(), 1);
    let result = parser.finish().unwrap();
    assert_eq!(result.threads, 4);
    assert_eq!(
        (result.visits_per_second, result.nn_evals_per_second),
        (44.95, 44.27)
    );
    assert_eq!(result.nn_max_batch_size, Some(2));
    assert_eq!(result.num_nn_server_threads_per_model, Some(2));
    // Even an unrelated recommended-looking row in the extra test section
    // cannot change the official search recommendation or add a measurement.
    assert!(parser.push(b"numSearchThreads = 8: 2 / 2 positions, visits/s = 1000 nnEvals/s = 900 (recommended)\n").unwrap().is_empty());
    assert_eq!(parser.finish().unwrap(), result);
    assert_eq!(parser.metrics().len(), 1);
}

#[test]
fn recommendations_require_explicit_advice_and_follow_java_bounds() {
    for (suffix, batch, server) in [
        (
            "ADDITIONAL RECOMMENDATION: faster. To use this, set nnMaxBatchSize = 65536 in your config.\nADDITIONAL RECOMMENDATION: faster. To use this, set numNNServerThreadsPerModel = 256 in your config.",
            Some(65536),
            Some(256),
        ),
        (
            "additional recommendation: faster. SET nnmaxbatchsize=2.\nadditional recommendation: faster. set numnnserverthreadspermodel=4.",
            Some(2),
            Some(4),
        ),
        (
            "ADDITIONAL RECOMMENDATION: faster. To use this, set nnMaxBatchSize = 0 in your config.",
            None,
            None,
        ),
        (
            "nnMaxBatchSize = 64\nnumNNServerThreadsPerModel = 8\nHalf batch size was not at least 3% faster, keeping default.\nADDITIONAL RECOMMENDATION: see the config.\nnumNNServerThreadsPerModel = 2",
            None,
            None,
        ),
    ] {
        let mut parser = BenchmarkParser::default();
        parser.push(RECORDING).unwrap();
        parser.push(suffix.as_bytes()).unwrap();
        // These cases isolate recommendation syntax and bounds. Neither item
        // is adopted, so no additional measurement is required for display.
        let result = parser.finish_with_ignored_settings(true, true).unwrap();
        assert_eq!(result.nn_max_batch_size, batch);
        assert_eq!(result.num_nn_server_threads_per_model, server);
    }
    for suffix in [
        "ADDITIONAL RECOMMENDATION: faster. set nnMaxBatchSize = 65537",
        "ADDITIONAL RECOMMENDATION: faster. set numNNServerThreadsPerModel = 257",
        "ADDITIONAL RECOMMENDATION: faster. set nnMaxBatchSize = 999999999999999999999999",
        "ADDITIONAL RECOMMENDATION: 2 NN server threads per GPU measured faster.\nnumNNServerThreadsPerModel = 257",
    ] {
        let mut parser = BenchmarkParser::default();
        parser.push(RECORDING).unwrap();
        parser.push(suffix.as_bytes()).unwrap();
        assert!(
            matches!(
                parser.finish(),
                Err(BenchmarkParseError::InvalidRecommendation(_))
            ),
            "{suffix}"
        );
    }
    assert!(matches!(
        BenchmarkParser::with_explicit_threads(4097),
        Err(BenchmarkParseError::InvalidExplicitThreads)
    ));
}

#[test]
fn ignores_additional_gpu_tests_and_keeps_search_metrics() {
    let mut parser = BenchmarkParser::default();
    parser
        .push(
            b"numSearchThreads = 10: 3 / 3 positions, visits/s = 386 nnEvals/s = 320\r\n\
        numSearchThreads = 10: +72 Elo (recommended)\n\
        Running additional tests of a few other settings at numSearchThreads = 10.\n\
        numSearchThreads = 10: 3 / 3 positions, visits/s = 420 nnEvals/s = 350\n",
        )
        .unwrap();
    assert_eq!(parser.finish().unwrap().visits_per_second, 386.0);
}

const MAIN: &str = "numSearchThreads = 4: 2 / 2 positions, visits/s = 15.75 nnEvals/s = 15.99\n\
numSearchThreads = 4: (recommended)\n\
Running additional tests of a few other settings at numSearchThreads = 4.\n\
Re-measuring the current recommendation as a baseline:\n\
numSearchThreads = 4: 2 / 2 positions, visits/s = 999 nnEvals/s = 888\n";

fn extra_result(extra: &str) -> Result<BenchmarkMetrics, BenchmarkParseError> {
    let mut parser = BenchmarkParser::default();
    parser.push(MAIN.as_bytes())?;
    parser.push(extra.as_bytes())?;
    parser.finish()
}

#[test]
fn rejected_additional_candidates_never_change_displayed_speed() {
    let result = extra_result(
        "Testing 2 NN server threads per GPU. This also uses more GPU memory:\n\
numSearchThreads = 4: 2 / 2 positions, visits/s = 1000 nnEvals/s = 900\n\
2 NN server threads per GPU was not at least 3% faster (measured +0.4%), keeping 1.\n\
Testing a max batch size of 2, half the search threads:\n\
numSearchThreads = 4: 2 / 2 positions, visits/s = 2000 nnEvals/s = 1900\n\
Half batch size was not at least 3% faster (measured +2.8%), keeping the default.\n",
    )
    .unwrap();
    assert_eq!(
        (result.visits_per_second, result.nn_evals_per_second),
        (15.75, 15.99)
    );
    assert_eq!(
        (
            result.nn_max_batch_size,
            result.num_nn_server_threads_per_model
        ),
        (None, None)
    );
}

#[test]
fn server_only_and_batch_only_adoption_choose_their_own_complete_rows() {
    let server = extra_result(
        "Testing 2 NN server threads per GPU. This also uses more GPU memory:\n\
numSearchThreads = 4: 2 / 2 positions, visits/s = 20 nnEvals/s = 19\n\
2 NN server threads per GPU was 26% faster, will recommend it.\n\
Testing a max batch size of 2, half the search threads:\n\
numSearchThreads = 4: 2 / 2 positions, visits/s = 18 nnEvals/s = 17\n\
Half batch size was not at least 3% faster, keeping the default.\n\
ADDITIONAL RECOMMENDATION: faster, set numNNServerThreadsPerModel = 2.\n",
    )
    .unwrap();
    assert_eq!(
        (server.visits_per_second, server.nn_evals_per_second),
        (20.0, 19.0)
    );
    let batch = extra_result(
        "Testing 2 NN server threads per GPU. This also uses more GPU memory:\n\
numSearchThreads = 4: 2 / 2 positions, visits/s = 14 nnEvals/s = 13\n\
2 NN server threads per GPU was not at least 3% faster, keeping 1.\n\
Testing a max batch size of 2, half the search threads:\n\
numSearchThreads = 4: 2 / 2 positions, visits/s = 44.95 nnEvals/s = 44.27\n\
Half batch size was 185% faster, will recommend it.\n\
ADDITIONAL RECOMMENDATION: faster, set nnMaxBatchSize = 2.\n",
    )
    .unwrap();
    assert_eq!(
        (batch.visits_per_second, batch.nn_evals_per_second),
        (44.95, 44.27)
    );
    assert_eq!(batch.num_nn_server_threads_per_model, None);
}

#[test]
fn later_unrelated_or_rejected_trials_do_not_replace_accepted_batch_speed() {
    let result = extra_result(
        "Testing a max batch size of 2, half the search threads:\n\
numSearchThreads = 4: 2 / 2 positions, visits/s = 44.95 nnEvals/s = 44.27\n\
Half batch size was 185% faster, will recommend it.\n\
Testing a max batch size of 4, another candidate:\n\
numSearchThreads = 4: 2 / 2 positions, visits/s = 99 nnEvals/s = 98\n\
Half batch size was not at least 3% faster, keeping the default.\n\
Testing an unrelated backend option:\n\
numSearchThreads = 4: 2 / 2 positions, visits/s = 999 nnEvals/s = 998\n\
ADDITIONAL RECOMMENDATION: faster, set nnMaxBatchSize = 2.\n",
    )
    .unwrap();
    assert_eq!(
        (result.visits_per_second, result.nn_evals_per_second),
        (44.95, 44.27)
    );
}

#[test]
fn missing_invalid_incomplete_or_wrong_thread_adopted_measurements_are_errors() {
    for extra in [
        "",
        "Testing an unknown setting:\nnumSearchThreads = 4: 2 / 2 positions, visits/s = 44 nnEvals/s = 43\n",
        "Testing a max batch size of 2, half the search threads:\nnumSearchThreads = 4: 1 / 2 positions, visits/s = 44 nnEvals/s = 43\n",
        "Testing a max batch size of 2, half the search threads:\nnumSearchThreads = 4: 2 / 2 positions, visits/s = NaN nnEvals/s = 43\n",
        "Testing a max batch size of 2, half the search threads:\nnumSearchThreads = 8: 2 / 2 positions, visits/s = 44 nnEvals/s = 43\n",
    ] {
        assert_eq!(
            extra_result(&format!(
                "{extra}ADDITIONAL RECOMMENDATION: faster, set nnMaxBatchSize = 2.\n"
            )),
            Err(BenchmarkParseError::MissingAdoptedMetrics)
        );
    }
}

#[test]
fn explicit_override_selection_matches_the_adopted_subset_and_retains_advice() {
    let mut parser = BenchmarkParser::default();
    parser
        .push(
            std::str::from_utf8(ADDITIONAL_REPLAY)
                .unwrap()
                .trim_end()
                .as_bytes(),
        )
        .unwrap();
    let same = parser
        .finish_with_overrides(Some(" 2 "), Some("2"))
        .unwrap();
    assert_eq!(
        (same.visits_per_second, same.nn_evals_per_second),
        (44.95, 44.27)
    );
    let main = parser.finish_with_overrides(Some("8"), Some("3")).unwrap();
    assert_eq!(
        (main.visits_per_second, main.nn_evals_per_second),
        (15.75, 15.99)
    );
    assert_eq!(
        (main.nn_max_batch_size, main.num_nn_server_threads_per_model),
        (Some(2), Some(2))
    );
    let server = parser.finish_with_overrides(Some("8"), None).unwrap();
    assert_eq!(
        (server.visits_per_second, server.nn_evals_per_second),
        (20.0, 19.0)
    );
    // The batch trial used the accepted server count. No row measured the batch
    // with baseline server settings, so it cannot justify a fabricated speed.
    assert_eq!(
        parser.finish_with_overrides(None, Some("3")),
        Err(BenchmarkParseError::MissingAdoptedMetrics)
    );
}

#[test]
fn multi_gpu_server_trial_uses_the_recommended_total_count() {
    let text = std::str::from_utf8(ADDITIONAL_REPLAY).unwrap().replace(
        "numNNServerThreadsPerModel = 2",
        "numNNServerThreadsPerModel = 4",
    );
    let mut parser = BenchmarkParser::default();
    parser.push(text.as_bytes()).unwrap();
    let result = parser.finish().unwrap();
    assert_eq!(result.num_nn_server_threads_per_model, Some(4));
    assert_eq!(
        (result.visits_per_second, result.nn_evals_per_second),
        (44.95, 44.27)
    );
}

#[test]
fn incomplete_or_invalid_output_never_invents_a_success() {
    for speed in ["NaN", "nan", "Infinity", "inf", "-1", "0", "1e999"] {
        let output = format!(
            "numSearchThreads = 6: 3 / 3 positions, visits/s = {speed} nnEvals/s = 43\nnumSearchThreads = 6: +70 Elo (recommended)"
        );
        let mut parser = BenchmarkParser::default();
        parser.push(output.as_bytes()).unwrap();
        assert_eq!(parser.finish(), Err(BenchmarkParseError::MissingMetrics));
    }
    for output in [
        "numSearchThreads = 6: +70 Elo (recommended)",
        "numSearchThreads = 6: 2 / 3 positions, visits/s = 44 nnEvals/s = 43\nnumSearchThreads = 6: +70 Elo (recommended)",
        "numSearchThreads = 6: 3 / 3 positions, visits/s = 44 nnEvals/s = 43",
        "numSearchThreads = 0: 3 / 3 positions, visits/s = 44 nnEvals/s = 43 (recommended)",
    ] {
        let mut parser = BenchmarkParser::default();
        parser.push(output.as_bytes()).unwrap();
        assert!(parser.finish().is_err(), "{output}");
    }
}

#[test]
fn conflicting_recommendations_invalid_utf8_and_oversized_fragments_fail() {
    let mut parser = BenchmarkParser::default();
    assert_eq!(
        parser.push(b"numSearchThreads = 3: (recommended)\nnumSearchThreads = 6: (recommended)\n"),
        Err(BenchmarkParseError::ConflictingRecommendations)
    );
    assert_eq!(
        BenchmarkParser::default().push(&[0xff, b'\n']),
        Err(BenchmarkParseError::InvalidUtf8)
    );
    assert_eq!(
        BenchmarkParser::default().push(&vec![b'x'; MAX_FRAGMENT_BYTES + 1]),
        Err(BenchmarkParseError::FragmentTooLong)
    );
}

#[test]
fn failure_after_valid_recommendation_is_rejected_but_finished_gpu_tuning_can_recover() {
    for failure in [
        "FATAL: failed to load model",
        "Segmentation fault: 11",
        "ERROR: invalid configuration",
        "Beginning GPU tuning\nERROR: no kernel",
    ] {
        let mut parser = BenchmarkParser::default();
        parser.push(RECORDING).unwrap();
        parser.push(failure.as_bytes()).unwrap();
        assert!(matches!(
            parser.finish(),
            Err(BenchmarkParseError::ReportedFailure(_))
        ));
    }
    let mut parser = BenchmarkParser::default();
    parser
        .push(b"Beginning GPU tuning\nERROR: invalid kernel\nDone tuning\n")
        .unwrap();
    parser.push(RECORDING).unwrap();
    assert_eq!(parser.finish().unwrap().threads, 6);
}

proptest! {
    #[test]
    fn real_recording_is_independent_of_byte_chunk_boundaries(chunk_size in 1usize..2048) {
        let mut all = BenchmarkParser::default();
        let expected_updates = all.push(RECORDING).unwrap();
        let expected_result = all.finish().unwrap();
        let mut chunked = BenchmarkParser::default();
        let mut updates = Vec::new();
        for chunk in RECORDING.chunks(chunk_size) {
            updates.extend(chunked.push(chunk).unwrap());
        }
        prop_assert_eq!(updates, expected_updates);
        prop_assert_eq!(chunked.finish().unwrap(), expected_result);
        prop_assert_eq!(chunked.metrics(), all.metrics());
    }

    #[test]
    fn metrics_round_trip_with_arbitrary_line_endings(
        threads in 1u32..4097,
        positions in 1u32..1000,
        visits in 1u32..1000000,
        nn_evals in 1u32..1000000,
        separator in prop_oneof![Just("\r"), Just("\n"), Just("\r\n")],
    ) {
        let text = format!("numSearchThreads = {threads}: {positions} / {positions} positions, visits/s = {visits} nnEvals/s = {nn_evals}{separator}numSearchThreads = {threads}: (baseline) (recommended)");
        let mut parser = BenchmarkParser::default();
        parser.push(text.as_bytes()).unwrap();
        prop_assert_eq!(parser.finish().unwrap(), BenchmarkMetrics {
            threads,
            completed_positions: positions,
            total_positions: positions,
            visits_per_second: f64::from(visits),
            nn_evals_per_second: f64::from(nn_evals),
            nn_max_batch_size: None,
            num_nn_server_threads_per_model: None,
        });
    }

    #[test]
    fn additional_advice_survives_chunking_and_line_endings(
        chunk_size in 1usize..2048,
        separator in prop_oneof![Just("\r"), Just("\n"), Just("\r\n")],
    ) {
        let text = std::str::from_utf8(ADDITIONAL_REPLAY).unwrap().replace('\n', separator);
        let mut parser = BenchmarkParser::default();
        let mut updates = Vec::new();
        for chunk in text.as_bytes().chunks(chunk_size) {
            updates.extend(parser.push(chunk).unwrap());
        }
        let result = parser.finish().unwrap();
        prop_assert_eq!(updates.len(), 3);
        prop_assert_eq!(result.threads, 4);
        prop_assert_eq!(result.visits_per_second, 44.95);
        prop_assert_eq!(result.nn_evals_per_second, 44.27);
        prop_assert_eq!(result.nn_max_batch_size, Some(2));
        prop_assert_eq!(result.num_nn_server_threads_per_model, Some(2));
        prop_assert_eq!(parser.metrics().len(), 1);
    }

    #[test]
    fn fixed_recording_survives_arbitrary_chunking(chunk_size in 1usize..2048) {
        let mut parser = BenchmarkParser::with_explicit_threads(4).unwrap();
        let mut updates = Vec::new();
        for chunk in FIXED_RECORDING.chunks(chunk_size) {
            updates.extend(parser.push(chunk).unwrap());
        }
        let result = parser.finish().unwrap();
        prop_assert_eq!(updates.len(), 3);
        prop_assert_eq!(result.nn_max_batch_size, Some(2));
        prop_assert_eq!(result.visits_per_second, 44.95);
    }

    #[test]
    fn arbitrary_output_does_not_panic(bytes in prop::collection::vec(any::<u8>(), 0..8192)) {
        let mut parser = BenchmarkParser::default();
        let _ = parser.push(&bytes);
        let _ = parser.finish();
    }
}
