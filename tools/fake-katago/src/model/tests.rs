#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;

#[test]
fn query_report_perspective_overrides_remote_default() {
    let mut input = json!({ "id":"perspective", "moves":[["B","D4"]], "rules":"chinese", "boardXSize":9, "boardYSize":9, "includeOwnership":true });
    let default = Query::parse(&input).unwrap();
    let black = default.response(1, 10, false, ReportPerspective::Black);
    let white = default.response(1, 10, false, ReportPerspective::ToPlay);
    assert_ne!(black["rootInfo"]["winrate"], white["rootInfo"]["winrate"]);
    input["overrideSettings"] = json!({ "reportAnalysisWinratesAs":"BLACK" });
    assert_eq!(
        Query::parse(&input)
            .unwrap()
            .response(1, 10, false, ReportPerspective::ToPlay),
        black
    );
    input["overrideSettings"] = json!({ "reportAnalysisWinratesAs":"WHITE" });
    assert_eq!(
        Query::parse(&input)
            .unwrap()
            .response(1, 10, false, ReportPerspective::Black),
        white
    );
}
