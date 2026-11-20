#![no_main]

use document::{Command, DocError, GameDocument};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(collection) = sgf::parse(data) else {
        return;
    };
    // AppCore opens the first game. Exercise that same document import and
    // semantic replay for every game in a collection, including all variations.
    for game in &collection.games {
        let Ok(mut document) = GameDocument::from_sgf(game) else {
            continue;
        };
        let _ = document.visit_saved_analysis::<DocError>(|_, position, _| {
            let _ = position.analysis_key();
            Ok(())
        });
        let _ = document.visit_saved_comparison_analysis::<DocError>(|_, position, _| {
            let _ = position.analysis_key();
            Ok(())
        });
        let _ = document.warnings();
        let _ = document.game_info();
        let _ = document.variation_tree();

        let mut pending = vec![document.root()];
        while let Some(id) = pending.pop() {
            let node = match document.node(id) {
                Ok(node) => node,
                Err(error) => panic!("imported node disappeared: {error}"),
            };
            pending.extend(node.children().iter().rev().copied());
            let _ = document.apply(Command::GoTo(id));
            // As in AppCore's document publication/focus path, replay applies
            // root rules/komi, moves, setup and PL. Errors are valid outcomes;
            // still visit descendants and siblings of an invalid record.
            if let Ok(position) = document.position_at(id) {
                let _ = position.analysis_key();
            }
            let _ = document.markup_at(id);
        }
    }
});
