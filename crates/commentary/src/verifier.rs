use std::collections::BTreeSet;

use crate::Evidence;

/// Whitelist checking is intentionally lexical: it cannot prove that prose is true.
/// Independent uppercase coordinate-like tokens are checked, including invalid GTP
/// spellings. Lowercase prose and word substrings do not count.
pub fn verify(answer: &str, evidence: &Evidence) -> Vec<String> {
    let bytes = answer.as_bytes();
    let mut issues = BTreeSet::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte.is_ascii_uppercase() && (cursor == 0 || !bytes[cursor - 1].is_ascii_alphanumeric())
        {
            let start = cursor;
            let mut end = cursor + 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > start + 1 && (end == bytes.len() || !bytes[end].is_ascii_alphanumeric()) {
                let coordinate = &answer[start..end];
                let valid_gtp = byte != b'I'
                    && bytes[start + 1] != b'0'
                    && answer[start + 1..end]
                        .parse::<u8>()
                        .is_ok_and(|row| (1..=25).contains(&row));
                if !valid_gtp {
                    issues.insert(format!(
                        "Invalid coordinate {coordinate}: not a valid GTP point."
                    ));
                } else if !evidence.allowed_coordinates.contains(coordinate) {
                    issues.insert(format!("Unsupported coordinate {coordinate}: not in supplied moves or named groups."));
                }
            }
        }
        if byte.is_ascii_digit()
            && (cursor == 0 || !(bytes[cursor - 1].is_ascii_digit() || bytes[cursor - 1] == b'.'))
        {
            let start = cursor;
            let mut end = cursor;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == b'.' {
                end += 1;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
            }
            let suffix = answer[end..].trim_start();
            if (suffix.starts_with('%') || suffix.starts_with('％'))
                && answer[start..end]
                    .parse::<f64>()
                    .is_ok_and(|number| number > 100.0)
            {
                issues.insert(format!(
                    "Impossible winrate percentage {}%.",
                    &answer[start..end]
                ));
            }
        }
        cursor += 1;
    }
    issues.into_iter().collect()
}
