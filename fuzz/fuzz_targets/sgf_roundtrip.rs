#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(collection) = sgf::parse(data) {
        let first = sgf::serialize(&collection);
        let reparsed = match sgf::parse(first.as_bytes()) {
            Ok(collection) => collection,
            Err(error) => panic!("serialized SGF did not parse: {error}"),
        };
        // Encoding declarations and recoverable syntax normalize on the first
        // write. There are no exceptions to stability after that write.
        assert_eq!(sgf::serialize(&reparsed), first);
    }
});
