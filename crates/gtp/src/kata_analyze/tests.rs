#![allow(clippy::unwrap_used)]
use super::*;
use proptest::prelude::*;

#[test]
fn golden_kata_analyze_reports() {
    // Hand-authored protocol fixture, based on the Java parser; no live service recording.
    let lines = include_str!("../../../../fixtures/zhizi/kata-analyze.txt");
    let reports = lines
        .lines()
        .map(parse)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(reports[0].moves.len(), 2);
    assert_eq!(
        reports[0].moves[0],
        Candidate {
            coordinate: "D4".into(),
            visits: 80,
            winrate: 0.65,
            score_lead: 3.5,
            prior: 0.4,
            order: 0,
            pv: vec!["D4".into(), "Q16".into(), "pass".into()]
        }
    );
    assert_eq!(reports[0].root.as_ref().unwrap().visits, 120);
    assert_eq!(reports[0].ownership, Some(vec![0.9, -0.8, 0.0, 0.1]));
    assert_eq!(reports[1].root.as_ref().unwrap().score_lead, 2.5);
    assert_eq!(reports[2].ownership, Some(vec![0.7, -0.5, 0.1, 0.0]));
}

#[test]
fn malformed_fields_are_typed_errors() {
    for line in [
        "",
        "info",
        "rootInfo visits",
        "ownership NaN",
        "ownership 1.1",
        "rootInfo visits -1 winrate 0.5 scoreLead 0",
        "rootInfo visits 1 winrate 2 scoreLead 0",
        "rootInfo visits 1 winrate 0.5 scoreLead inf",
        "info move I4 visits 2 winrate 0.5 scoreLead 0 prior 0.3 order 0 pv I4",
        "info move D4 visits 2 winrate 0.5 scoreLead 0 prior 0.3 order 0 pv",
    ] {
        assert!(parse(line).is_err(), "{line}");
    }
}

proptest! {
    #[test]
    fn generated_report_roundtrips(visits in 0u64..u64::MAX, rate in 0u32..=10000, score in -10000i32..10000, order in 0u32..1000) {
        let line = format!("info move D4 visits {visits} winrate {} scoreLead {} prior 0.3 order {order} pv D4 pass rootInfo visits {visits} winrate {} scoreLead {}", f64::from(rate)/10000.0, f64::from(score)/10.0, f64::from(rate)/10000.0, f64::from(score)/10.0);
        let report = parse(&line).unwrap();
        prop_assert_eq!(report.moves[0].visits, visits);
        prop_assert_eq!(report.moves[0].order, order);
        prop_assert_eq!(report.moves[0].winrate, f64::from(rate)/10000.0);
        prop_assert_eq!(report.root.unwrap().score_lead, f64::from(score)/10.0);
    }
    #[test]
    fn arbitrary_input_never_panics(line in ".{0,4096}") { let _ = parse(&line); }
}
