#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    // Exercise both a single malformed line and state across complete sessions.
    let _ = readboard::Decoder::default().push_line(text);
    let mut decoder = readboard::Decoder::default();
    for line in text.split_inclusive('\n') {
        let _ = decoder.push_line(line);
    }
});
