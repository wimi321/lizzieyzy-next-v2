use super::{BridgeError, query::to_play};
use gtp::kata_analyze::Report;
use katago_protocol::{AnalysisQuery, Player};
use serde_json::{Value, json};

pub(super) fn merge(latest: &mut Report, report: Report) {
    if !report.moves.is_empty() {
        latest.moves = report.moves;
    }
    if report.root.is_some() {
        latest.root = report.root;
    }
    if report.ownership.is_some() {
        latest.ownership = report.ownership;
    }
}
pub(super) fn response(
    query: &AnalysisQuery,
    turn: u32,
    report: &Report,
    during: bool,
) -> Result<Option<Value>, BridgeError> {
    let Some(root) = &report.root else {
        return Ok(None);
    };
    if report.moves.is_empty() {
        return Ok(None);
    }
    let white = to_play(query, turn) == Player::White;
    let rate = |v: f64| if white { 1.0 - v } else { v };
    let signed = |v: f64| if white { -v } else { v };
    let moves = report.moves.iter().map(|candidate| json!({
        "move": candidate.coordinate, "visits": candidate.visits, "winrate": rate(candidate.winrate),
        "scoreLead": signed(candidate.score_lead), "prior": candidate.prior, "order": candidate.order, "pv": candidate.pv,
    })).collect::<Vec<_>>();
    let mut output = json!({
        "id": query.id, "turnNumber": turn, "isDuringSearch": during, "moveInfos": moves,
        "rootInfo": { "visits": root.visits, "winrate": rate(root.winrate), "scoreLead": signed(root.score_lead),
            "utility": signed(root.utility), "currentPlayer": to_play(query, turn) },
    });
    if let Some(ownership) = &report.ownership {
        if ownership.len() != usize::from(query.board_x_size) * usize::from(query.board_y_size) {
            return Err(BridgeError::MalformedReport);
        }
        if query.include_ownership.unwrap_or(false) {
            output["ownership"] = json!(ownership.iter().map(|v| signed(*v)).collect::<Vec<_>>());
        }
    }
    Ok(Some(output))
}
