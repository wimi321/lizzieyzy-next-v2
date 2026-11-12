#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;

fn document(text: &str) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(text.as_bytes()).unwrap().games[0]).unwrap()
}

fn inspect(document: &GameDocument) -> (Work, Vec<NodeId>) {
    let mut seen = Vec::new();
    let work = document
        .visit_saved_analysis_inner::<DocError>(|id, position, saved| {
            assert_eq!(*position, document.position_at(id).unwrap());
            assert_eq!(Some(saved), document.saved_analysis(id).unwrap());
            seen.push(id);
            Ok(())
        })
        .unwrap();
    (work, seen)
}

#[test]
fn no_primary_analysis_means_no_position_computation() {
    let mut text = "(;SZ[5]LZ2[secondary]".to_owned();
    for _ in 0..1000 {
        text.push_str(";B[];W[]");
    }
    text.push_str("(;B[aa])(;W[bb]))");
    let (work, seen) = inspect(&document(&text));
    assert_eq!(work.root_replays, 0);
    assert!(work.applied.is_empty());
    assert!(seen.is_empty());
}

#[test]
fn unanalyzed_tails_and_branches_do_not_increase_replay_work() {
    let payload = "LZ[test 45 10 2]";
    let short = format!("(;SZ[5];B[aa]{payload})");
    let mut long = format!("(;SZ[5](;B[aa]{payload}");
    for _ in 0..1000 {
        long.push_str(";W[];B[]");
    }
    long.push(')');
    for _ in 0..100 {
        long.push_str("(;W[bb];B[cc];W[dd])");
    }
    long.push(')');
    let (short_work, short_seen) = inspect(&document(&short));
    let (long_work, long_seen) = inspect(&document(&long));
    assert_eq!(long_work.root_replays, 1);
    assert_eq!(short_work.root_replays, long_work.root_replays);
    assert_eq!(short_work.applied, long_work.applied);
    assert_eq!(long_work.applied, [NodeId(0), NodeId(1)]);
    assert_eq!(short_seen, long_seen);
}

#[test]
fn analyzed_nodes_share_one_replay_and_match_setup_pass_and_variation_semantics() {
    let doc = document(
        "(;SZ[5]AB[aa]PL[W]LZ[test 45 10 2];W[]LZ[test 45 10 2](;B[bb]LZ[test 45 10 2];AE[aa]PL[B]LZ[test 45 10 2])(;B[cc]LZ[test 45 10 2]))",
    );
    let (work, seen) = inspect(&doc);
    assert_eq!(work.root_replays, 1);
    assert_eq!(
        seen,
        [NodeId(0), NodeId(1), NodeId(2), NodeId(3), NodeId(4)]
    );
    let mut applied = work.applied;
    applied.sort();
    assert_eq!(applied, seen);
}

#[test]
fn bad_positions_skip_their_descendants_but_keep_valid_siblings() {
    let doc =
        document("(;SZ[5];B[aa](;W[aa]LZ[test 45 10];B[]LZ[test 45 10])(;W[bb]LZ[test 45 10]))");
    let (work, seen) = inspect(&doc);
    assert_eq!(seen, [NodeId(4)]);
    assert!(!work.applied.contains(&NodeId(3)));
    assert_eq!(work.root_replays, 1);
}
