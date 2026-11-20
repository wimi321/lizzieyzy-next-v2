#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(response) = fox_kifu::parse_json_response(bytes) {
        let _ = fox_kifu::parse_account_response(&response, "");
        let _ = fox_kifu::parse_page_response(&response, "0");
        let _ = fox_kifu::parse_download_response(&response);
        let _ = fox_kifu::tencent::parse_page_response(&response, "0");
        let _ = fox_kifu::tencent::parse_download_response(&response);
    }
    // Exercise SGF cleanup directly too, so mutations need not preserve JSON
    // escaping and the surrounding successful API response to reach it.
    if let Ok(text) = std::str::from_utf8(bytes) {
        let _ = fox_kifu::normalize_sgf(text);
    }
});
