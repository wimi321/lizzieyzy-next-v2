#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "../benches/support/mod.rs"]
mod support;

use std::fs;

use document::{DocError, GameDocument};
use support::{Limits, REPETITIONS, Report, Scenario, ScenarioResult, WARMUPS};

fn report() -> Report {
    Report {
        platform: "macos".into(),
        architecture: "aarch64".into(),
        warmups: WARMUPS,
        repetitions: REPETITIONS,
        results: Scenario::ALL
            .into_iter()
            .map(|scenario| {
                ScenarioResult::new(
                    scenario,
                    vec![60.0, 10.0, 90.0, 20.0, 30.0, 80.0, 70.0],
                    if scenario.has_memory() {
                        vec![1_000, 1_001, 1_002, 1_003, 1_004, 1_005, 2_000]
                    } else {
                        Vec::new()
                    },
                )
                .unwrap()
            })
            .collect(),
    }
}

fn limits() -> Limits {
    let scenarios: serde_json::Map<_, _> = Scenario::ALL
        .into_iter()
        .map(|scenario| {
            (
                scenario.as_str().into(),
                serde_json::json!({
                    "median_ms": 60.0,
                    "peak_rss_bytes": scenario.has_memory().then_some(2_000)
                }),
            )
        })
        .collect();
    serde_json::from_value(serde_json::json!({
        "macos": scenarios,
        "linux": scenarios,
        "windows": scenarios
    }))
    .unwrap()
}

#[test]
fn statistics_use_the_middle_sample_and_largest_peak_without_reordering_samples() {
    let report = report();
    let result = &report.results[0];
    assert_eq!(result.median_ms, 60.0);
    assert_eq!(result.max_ms, 90.0);
    assert_eq!(
        result.samples_ms,
        [60.0, 10.0, 90.0, 20.0, 30.0, 80.0, 70.0]
    );
    assert_eq!(result.peak_rss_bytes, Some(2_000));
    assert_eq!(report.results[2].peak_rss_bytes, None);
    for samples in [vec![], vec![0.0; 6], vec![0.0; 8]] {
        assert!(ScenarioResult::new(Scenario::EngineReady, samples, vec![]).is_err());
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        let mut samples = vec![1.0; REPETITIONS];
        samples[3] = value;
        assert!(ScenarioResult::new(Scenario::EngineReady, samples, vec![]).is_err());
    }
    assert!(ScenarioResult::new(Scenario::EngineReady, vec![0.0; 7], vec![]).is_ok());
}

#[test]
fn memory_samples_allow_unavailable_but_reject_partial_zero_or_unmeasured_scenarios() {
    let unavailable = ScenarioResult::new(Scenario::FirstAnalysis, vec![1.0; 7], vec![]).unwrap();
    assert_eq!(unavailable.peak_rss_bytes, None);
    assert!(unavailable.peak_rss_samples_bytes.is_empty());
    for samples in [vec![100; 1], vec![100; 6], vec![100; 8], vec![0; 7]] {
        assert!(ScenarioResult::new(Scenario::FirstAnalysis, vec![1.0; 7], samples).is_err());
    }
    assert!(ScenarioResult::new(Scenario::EngineReady, vec![1.0; 7], vec![100; 7]).is_err());
}

#[test]
fn limits_gate_median_and_peak_but_not_the_timing_outlier() {
    let mut report = report();
    assert!(limits().check(&report).unwrap().is_empty());
    report.results[0] = ScenarioResult::new(
        Scenario::OpenReviewedGame,
        vec![10.0, 20.0, 30.0, 60.1, 70.0, 80.0, 90.0],
        vec![2_001; 7],
    )
    .unwrap();
    let violations = limits().check(&report).unwrap();
    assert_eq!(violations.len(), 2);
    assert!(violations[0].contains("open_reviewed_game median 60.100"));
    assert!(violations[1].contains("open_reviewed_game peak RSS 2001"));

    // A slow single measurement is reported, but the gate still uses median.
    report.results[0] = ScenarioResult::new(
        Scenario::OpenReviewedGame,
        vec![10.0, 20.0, 30.0, 60.0, 70.0, 80.0, 9_000.0],
        vec![2_000; 7],
    )
    .unwrap();
    assert!(limits().check(&report).unwrap().is_empty());
}

#[test]
fn limits_select_the_platform_and_reject_incomplete_or_invalid_configurations() {
    let mut limits = limits();
    limits
        .0
        .get_mut("linux")
        .unwrap()
        .get_mut(&Scenario::EngineReady)
        .unwrap()
        .median_ms = 50.0;
    let mut report = report();
    assert!(limits.check(&report).unwrap().is_empty());
    report.platform = "linux".into();
    assert_eq!(limits.check(&report).unwrap().len(), 1);
    report.platform = "unknown".into();
    assert!(limits.check(&report).is_err());

    let mut missing = limits.clone();
    missing.0.remove("windows");
    assert!(missing.validate().is_err());
    let mut missing = limits.clone();
    missing
        .0
        .get_mut("macos")
        .unwrap()
        .remove(&Scenario::FirstAnalysis);
    assert!(missing.validate().is_err());
    let mut invalid = limits.clone();
    invalid
        .0
        .get_mut("macos")
        .unwrap()
        .get_mut(&Scenario::FirstAnalysis)
        .unwrap()
        .peak_rss_bytes = Some(0);
    assert!(invalid.validate().is_err());
    for value in [0.0, -1.0, 49.999, f64::NAN, f64::INFINITY] {
        let mut invalid = limits.clone();
        invalid
            .0
            .get_mut("macos")
            .unwrap()
            .get_mut(&Scenario::FirstAnalysis)
            .unwrap()
            .median_ms = value;
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn null_memory_limit_skips_comparison_with_or_without_a_measurement() {
    let mut limits = limits();
    limits
        .0
        .get_mut("macos")
        .unwrap()
        .get_mut(&Scenario::OpenReviewedGame)
        .unwrap()
        .peak_rss_bytes = None;
    for samples in [vec![2_001; REPETITIONS], vec![]] {
        let mut report = report();
        report.results[0] =
            ScenarioResult::new(Scenario::OpenReviewedGame, vec![60.0; REPETITIONS], samples)
                .unwrap();
        assert!(limits.check(&report).unwrap().is_empty());
    }
}

#[test]
fn configured_memory_limit_reports_a_missing_measurement_as_a_failure() {
    let mut report = report();
    report.results[0] =
        ScenarioResult::new(Scenario::OpenReviewedGame, vec![60.0; REPETITIONS], vec![]).unwrap();
    assert!(report.validate().is_ok());
    let violations = limits().check(&report).unwrap();
    assert_eq!(
        violations,
        ["open_reviewed_game peak RSS is unavailable but a limit of 2000 bytes is configured"]
    );
}

#[test]
fn latency_limits_accept_the_50_ms_floor_and_check_its_boundary() {
    let mut limits = limits();
    let mut report = report();
    for result in &mut report.results {
        *result = ScenarioResult::new(
            result.scenario,
            vec![50.0; REPETITIONS],
            result.peak_rss_samples_bytes.clone(),
        )
        .unwrap();
    }
    for limit in limits.0.get_mut("macos").unwrap().values_mut() {
        limit.median_ms = 50.0;
    }
    assert!(limits.validate().is_ok());
    assert!(limits.check(&report).unwrap().is_empty());
    report.results[0] = ScenarioResult::new(
        Scenario::OpenReviewedGame,
        vec![50.001; REPETITIONS],
        vec![2_000; REPETITIONS],
    )
    .unwrap();
    assert_eq!(limits.check(&report).unwrap().len(), 1);
    limits
        .0
        .get_mut("macos")
        .unwrap()
        .get_mut(&Scenario::OpenReviewedGame)
        .unwrap()
        .median_ms = 49.999;
    assert!(limits.validate().is_err());
}

#[test]
fn windows_json_and_markdown_explicitly_record_memory_as_unavailable() {
    let mut report = report();
    report.platform = "windows".into();
    report.architecture = "x86_64".into();
    for result in &mut report.results {
        *result = ScenarioResult::new(result.scenario, result.samples_ms.clone(), vec![]).unwrap();
    }
    let mut limits = limits();
    for limit in limits.0.get_mut("windows").unwrap().values_mut() {
        limit.peak_rss_bytes = None;
    }
    assert!(limits.check(&report).unwrap().is_empty());

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("performance.json");
    report.write_json(&path).unwrap();
    assert_eq!(Report::read_json(&path).unwrap(), report);
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for result in json["results"].as_array().unwrap() {
        assert_eq!(result.get("peak_rss_bytes"), Some(&serde_json::Value::Null));
        assert_eq!(result["peak_rss_samples_bytes"], serde_json::json!([]));
    }
    let markdown = report.markdown_table();
    assert!(markdown.contains("Windows 不测"));
    for scenario in Scenario::ALL {
        assert!(markdown.contains(&format!("| {} | 60.000 | 90.000 | — |", scenario.label())));
    }
}

#[test]
fn report_json_round_trip_validates_complete_samples_and_derived_statistics() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("nested/performance.json");
    let report = report();
    report.write_json(&path).unwrap();
    assert_eq!(Report::read_json(&path).unwrap(), report);

    let mut tampered = report.clone();
    tampered.results[0].median_ms = 0.1;
    fs::write(&path, serde_json::to_vec(&tampered).unwrap()).unwrap();
    assert!(Report::read_json(&path).is_err());
    tampered = report.clone();
    tampered.results[0].peak_rss_bytes = Some(1);
    assert!(tampered.write_json(&path).is_err());
    tampered = report.clone();
    tampered.results.pop();
    assert!(tampered.validate().is_err());
    tampered = report.clone();
    tampered.results[0] = tampered.results[1].clone();
    assert!(tampered.validate().is_err());
    tampered = report;
    tampered.warmups = 0;
    assert!(tampered.validate().is_err());
    fs::write(&path, b"not json").unwrap();
    assert!(Report::read_json(&path).is_err());
    assert!(Report::read_json(&temp.path().join("absent.json")).is_err());
}

#[test]
fn limits_json_round_trip_and_summary_append_use_real_files() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("limits.json");
    fs::write(&path, serde_json::to_vec(&limits()).unwrap()).unwrap();
    assert!(
        Limits::read_json(&path)
            .unwrap()
            .check(&report())
            .unwrap()
            .is_empty()
    );
    fs::write(&path, b"{}").unwrap();
    assert!(Limits::read_json(&path).is_err());
    fs::write(&path, b"invalid json").unwrap();
    assert!(Limits::read_json(&path).is_err());

    let path = temp.path().join("summary.md");
    fs::write(&path, "Existing summary\n").unwrap();
    let report = report();
    report.append_summary(&path).unwrap();
    report.append_summary(&path).unwrap();
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        format!(
            "Existing summary\n{}{}",
            report.markdown_table(),
            report.markdown_table()
        )
    );
    assert!(
        report
            .markdown_table()
            .contains("| Engine ready | 60.000 | 90.000 | — |")
    );
}

fn count_nodes(tree: &sgf::GameTree) -> usize {
    tree.sequence.len() + tree.children.iter().map(count_nodes).sum::<usize>()
}

#[test]
fn reviewed_generator_is_deterministic_with_the_required_tree_and_importable_analysis() {
    let text = support::reviewed_game().unwrap();
    assert_eq!(text, support::reviewed_game().unwrap());
    let collection = sgf::parse(text.as_bytes()).unwrap();
    assert!(collection.warnings.is_empty());
    let tree = &collection.games[0];
    assert_eq!(count_nodes(tree), 701);
    let mut current = tree;
    let mut main_moves = 0;
    let mut branches = 0;
    loop {
        for node in &current.sequence {
            if node
                .properties
                .iter()
                .any(|property| matches!(property.identifier.as_str(), "B" | "W"))
            {
                main_moves += 1;
                for key in ["C", "LZ"] {
                    assert!(node.properties.iter().any(
                        |property| property.identifier == key && !property.values[0].is_empty()
                    ));
                }
            }
        }
        for branch in current.children.iter().skip(1) {
            branches += 1;
            assert_eq!(count_nodes(branch), 10);
        }
        match current.children.first() {
            Some(child) => current = child,
            None => break,
        }
    }
    assert_eq!(main_moves, 400);
    assert_eq!(branches, 30);
    let document = GameDocument::from_sgf(tree).unwrap();
    assert!(document.warnings().is_empty());
    let mut imported = 0;
    document
        .visit_saved_analysis::<DocError>(|_, position, analysis| {
            imported += 1;
            assert_eq!(position.board.size().width, 19);
            assert_eq!(position.board.size().height, 19);
            assert_eq!(analysis.visits, 1_000);
            assert!(!analysis.candidates.is_empty());
            assert_eq!(analysis.ownership.as_ref().unwrap().len(), 361);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        imported, 700,
        "every generated move is legal and its LZ is imported"
    );
}

#[test]
fn large_generator_is_deterministic_has_twenty_thousand_nodes_and_many_branches() {
    let text = support::large_tree().unwrap();
    assert_eq!(text, support::large_tree().unwrap());
    let collection = sgf::parse(text.as_bytes()).unwrap();
    assert!(collection.warnings.is_empty());
    let tree = &collection.games[0];
    assert_eq!(count_nodes(tree), 20_000);
    let document = GameDocument::from_sgf(tree).unwrap();
    let nodes = document.variation_tree().unwrap();
    assert_eq!(nodes.len(), 20_000);
    assert!(nodes.iter().filter(|node| node.children.len() > 1).count() > 1_000);
    // Exercise public document replay on a spread of paths rather than assert
    // generation internals or put any wall-clock condition in cargo test.
    for node in nodes.iter().step_by(137) {
        let position = document.position_at(node.id).unwrap();
        assert_eq!(position.board.size().width, 19);
        assert_eq!(position.board.size().height, 19);
    }
}
