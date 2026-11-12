#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::actor::tests::{actor, snapshot};

#[test]
fn alias_nodes_restore_the_deepest_payload_regardless_of_insertion_order() {
    let (actor, _) = actor();
    let snapshot = snapshot(&actor, 500, 0.7);
    let deep = StoredAnalysis {
        saved: None,
        node_id: NodeId(0),
        key: snapshot.key,
        response: snapshot.response,
    };
    let mut shallow = deep.clone();
    shallow.node_id = NodeId(1);
    shallow.response.root_info.visits = 1;
    shallow.response.root_info.winrate = 0.1;
    for nodes in [[&deep, &shallow], [&shallow, &deep]] {
        let restored = best_by_key(nodes.into_iter());
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[&deep.key].response, deep.response);
    }
}
