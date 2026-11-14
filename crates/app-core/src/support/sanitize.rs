//! Privacy categories follow Java's ExportSanitizer, PersistenceSanitizer,
//! SyncDiagnosticsExportSanitizer and ConfigExportProjection. Unlike Java's
//! stable aliases, ambiguous free text is omitted as a whole line. Support data
//! must fail closed; preserving an arbitrary command or account is unnecessary.

use serde_json::{Map, Value};

const REDACTED: &str = "<redacted>";

pub(super) fn json(value: Value) -> Value {
    value_for_key(value, "")
}

fn value_for_key(value: Value, key: &str) -> Value {
    if private_key(key) {
        return Value::String(REDACTED.into());
    }
    match value {
        Value::Object(mut fields) => {
            if fields.contains_key("privacy") && fields.contains_key("value") {
                return match fields.get("privacy").and_then(Value::as_str) {
                    Some("safe") => json(fields.remove("value").unwrap_or(Value::Null)),
                    _ => Value::String(REDACTED.into()),
                };
            }
            let mut safe = Map::new();
            let mut next_key = 0_u64;
            for (name, value) in fields {
                let safe_name = if safe_key(&name) {
                    name.clone()
                } else {
                    loop {
                        next_key += 1;
                        let candidate = format!("redacted-field-{next_key}");
                        if !safe.contains_key(&candidate) {
                            break candidate;
                        }
                    }
                };
                safe.insert(safe_name, value_for_key(value, &name));
            }
            Value::Object(safe)
        }
        Value::Array(values) => {
            Value::Array(values.into_iter().map(|v| value_for_key(v, key)).collect())
        }
        Value::String(value) => Value::String(text(&value)),
        value => value,
    }
}

fn safe_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
        && text(key) == key
}

fn private_key(key: &str) -> bool {
    let key: String = key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    [
        "password",
        "passwd",
        "token",
        "secret",
        "cookie",
        "authorization",
        "credential",
        "machinekey",
        "apikey",
        "account",
        "nickname",
        "username",
        "session",
        "room",
        "path",
        "file",
        "directory",
        "url",
    ]
    .iter()
    .any(|word| key.contains(word))
        || matches!(
            key.as_str(),
            "pw" | "uid"
                | "id"
                | "player"
                | "name"
                | "program"
                | "model"
                | "config"
                | "extraargs"
                | "arguments"
                | "foxhistory"
                | "tencenthistory"
                | "mainengineid"
                | "sgf"
                | "moves"
                | "initialstones"
        )
}

/// A support archive is not a log replay: URLs, paths (including usernames),
/// account/nickname/session identifiers, credentials and SGF are never exported.
/// We also apply this to local persistence so verbose logs do not retain secrets.
pub(super) fn text(value: &str) -> String {
    let Some(canonical) = canonicalize(value) else {
        return "<redaction-failed>".into();
    };
    if canonical.contains("(;") {
        return "<redacted-sgf>\n".into();
    }
    let lower = canonical.to_ascii_lowercase();
    let sensitive = [
        "password",
        "passwd",
        "token",
        "secret",
        "cookie",
        "authorization",
        "credential",
        "machinekey",
        "machine_key",
        "machine-key",
        "apikey",
        "api_key",
        "api-key",
        "nickname",
        "username",
        "account",
        "bearer ",
        "basic ",
        "live-room:",
        "unite-board:",
        "sgf=",
        "\"sgf\"",
        "moves=",
        "\"moves\"",
        "initialstones",
    ]
    .iter()
    .any(|word| lower.contains(word));
    let identifier = lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|word| {
            matches!(
                word,
                "uid" | "player" | "room" | "roomid" | "session" | "pw"
            )
        });
    // A slash may be an absolute/relative path or a URL, including UNC/WSL.
    // Removing the full line also handles quoted paths containing spaces.
    if sensitive || identifier || canonical.contains('/') || canonical.contains('\\') {
        if canonical.ends_with('\n') {
            "<redacted-sensitive-line>\n".into()
        } else {
            "<redacted-sensitive-line>".into()
        }
    } else {
        canonical
    }
}

/// Known single-line records can be sanitized independently without losing the
/// other recent events. Unframed or multiline legacy data remains one unit, so a
/// quoted multiline credential cannot leak its continuation as a separate line.
pub(super) fn log_file(value: &str) -> String {
    if !value.lines().all(record_line) {
        return text(value);
    }
    value.split_inclusive('\n').map(text).collect()
}

fn record_line(line: &str) -> bool {
    if matches!(
        line,
        "<redacted-sensitive-line>" | "<redacted-sgf>" | "<redaction-failed>"
    ) {
        return true;
    }
    let mut fields = line.split_whitespace();
    let time = fields.next().unwrap_or_default();
    let level = fields.next().unwrap_or_default();
    let target = fields.next().unwrap_or_default();
    !time.is_empty()
        && time.len() <= 20
        && time.bytes().all(|c| c.is_ascii_digit())
        && matches!(level, "ERROR" | "WARN" | "INFO" | "DEBUG" | "TRACE")
        && !target.is_empty()
        && target
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_:-.".contains(&c))
}

fn canonicalize(value: &str) -> Option<String> {
    let mut value = value.to_owned();
    for _ in 0..4 {
        let (next, changed) = decode(&value);
        value = next;
        if !changed {
            return Some(value);
        }
    }
    // Excessively nested syntax is deliberately excluded rather than copied.
    let (_, changed) = decode(&value);
    (!changed).then_some(value)
}

fn decode(value: &str) -> (String, bool) {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut changed = false;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[index + 1..index + 3])
            && let Ok(c) = u8::from_str_radix(hex, 16)
            && c.is_ascii()
        {
            output.push(char::from(c));
            index += 3;
            changed = true;
            continue;
        }
        if bytes[index] == b'\\' && index + 1 < bytes.len() {
            if matches!(bytes[index + 1], b'u' | b'U')
                && index + 5 < bytes.len()
                && let Ok(hex) = std::str::from_utf8(&bytes[index + 2..index + 6])
                && let Ok(c) = u32::from_str_radix(hex, 16)
                && let Some(c) = char::from_u32(c)
            {
                output.push(c);
                index += 6;
                changed = true;
                continue;
            }
            let escaped = match bytes[index + 1] {
                b'n' | b'r' => Some('\n'),
                b't' => Some(' '),
                b'"' => Some('"'),
                b'\'' => Some('\''),
                b'/' => Some('/'),
                b'\\' => Some('\\'),
                _ => None,
            };
            if let Some(c) = escaped {
                output.push(c);
                index += 2;
                changed = true;
                continue;
            }
        }
        if let Some(c) = value[index..].chars().next() {
            output.push(c);
            index += c.len_utf8();
        }
    }
    (output, changed)
}
