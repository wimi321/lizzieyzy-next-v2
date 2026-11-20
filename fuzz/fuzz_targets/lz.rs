#![no_main]

use go_core::{BoardSize, Color};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let [width, height, player, payload @ ..] = data else {
        return;
    };
    // Three input bytes vary all supported rectangular sizes and both colors;
    // the remaining bytes are a raw LZ value or a fixture SGF containing LZ.
    let Ok(size) = BoardSize::new(2 + width % 24, 2 + height % 24) else {
        return;
    };
    let player = if player & 1 == 0 {
        Color::Black
    } else {
        Color::White
    };
    if let Ok(payload) = std::str::from_utf8(payload) {
        let _ = document::lz::parse(payload, size, player);
    }
    // Use the production SGF decoder to extract seeds, including escaped and
    // legacy-encoded values. No harness-specific SGF/property parser is needed.
    if let Ok(collection) = sgf::parse(payload) {
        let mut pending: Vec<_> = collection.games.iter().collect();
        while let Some(tree) = pending.pop() {
            pending.extend(&tree.children);
            for node in &tree.sequence {
                for property in &node.properties {
                    if matches!(
                        property.identifier.as_str(),
                        "LZ" | "LZOP" | "LZ2" | "LZOP2"
                    ) {
                        for value in &property.values {
                            let _ = document::lz::parse(value, size, player);
                        }
                    }
                }
            }
        }
    }
});
