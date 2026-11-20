#![no_main]

use go_core::BoardSize;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    // The complete response and incremental decoder are both production APIs.
    let _ = gtp::parse_response(text);
    let mut decoder = gtp::Decoder::default();
    let size = BoardSize {
        width: 2 + data.first().copied().unwrap_or(17) % 24,
        height: 2 + data.get(1).copied().unwrap_or(17) % 24,
    };
    let _ = gtp::parse_genmove(text, size);
    let _ = gtp::kata_analyze::parse(text);
    for line in text.lines() {
        if let Ok(Some(response)) = decoder.push_line(line) {
            let _ = gtp::parse_genmove(&response.body, size);
        }
        let _ = gtp::parse_genmove(line, size);
        let _ = gtp::kata_analyze::parse(line);
    }
    let _ = decoder.finish();
});
