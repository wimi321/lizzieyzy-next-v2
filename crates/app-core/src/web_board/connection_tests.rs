use super::*;
use crate::web_board::{WebAnalysisUpdate, WebFullState, WebTrialState};

fn analysis(revision: u64) -> WebServerMessage {
    WebServerMessage::AnalysisUpdate(WebAnalysisUpdate {
        revision,
        node_id: 0,
        board: None,
        analysis: None,
    })
}

#[test]
fn mailbox_keeps_latest_per_kind_and_full_state_invalidates_pending_analysis() {
    let mut outbound = Outbound::default();
    outbound.push(analysis(1));
    outbound.push(analysis(2));
    outbound.push(WebServerMessage::TrialState(WebTrialState::default()));
    assert_eq!(outbound.next(&[0; KINDS], true).unwrap().2, analysis(2));
    assert_eq!(outbound.next(&[0; KINDS], false).unwrap().0, 5);
    outbound.push(WebServerMessage::FullState(WebFullState {
        revision: 3,
        node_id: 0,
        board: None,
        analysis: None,
    }));
    assert!(!outbound.pending_analysis(&[0; KINDS]));
    let (kind, sequence, _) = outbound.next(&[0; KINDS], false).unwrap();
    assert_eq!(kind, 1);
    let mut seen = [0; KINDS];
    seen[kind] = sequence;
    let (kind, sequence, _) = outbound.next(&seen, false).unwrap();
    assert_eq!(kind, 5);
    seen[kind] = sequence;
    assert!(outbound.next(&seen, true).is_none());
    outbound.push(analysis(4));
    outbound.push(analysis(5));
    assert_eq!(outbound.next(&seen, true).unwrap().2, analysis(5));
    assert!(outbound.next(&seen, false).is_none());
}
