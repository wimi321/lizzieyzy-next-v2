//! Tygem GIB import into the ordinary SGF tree representation.
//!
//! INI placement, GONGJE, STO colors and implicit SKI colors follow the Java
//! GIBParser. Rank/result/date fields omitted by that parser use the established
//! GIB tags documented by Sabaki's src/modules/fileformats/gib.js.

use crate::{Collection, ErrorKind, GameTree, Node, Property, SgfError, Warning, WarningKind};

const HANDICAP_POINTS: [(u8, u8); 9] = [
    (3, 15),
    (15, 3),
    (15, 15),
    (3, 3),
    (3, 9),
    (15, 9),
    (9, 3),
    (9, 15),
    (9, 9),
];

/// Import GIB with the same deterministic byte-decoding policy as SGF.
///
/// Invalid records return a positioned error; no partly imported tree escapes.
/// The returned tree is a normal, UTF-8-saveable SGF game, with setup stones
/// represented by AB rather than artificial black moves.
pub fn parse_gib(bytes: &[u8]) -> Result<Collection, SgfError> {
    let decoded = crate::encoding::decode(bytes)?;
    // A damaged SGF may contain GIB-looking lines inside an unclosed comment.
    // Its explicit format marker must not be reinterpreted by byte import.
    if decoded.text.trim_start().starts_with('(') {
        return Err(error(0, "SGF input is not a GIB record"));
    }
    let mut root = Node::default();
    for (key, value) in [
        ("FF", "4"),
        ("GM", "1"),
        ("SZ", "19"),
        ("CA", "UTF-8"),
        ("PB", "Player 2"),
        ("PW", "Player 1"),
    ] {
        put(&mut root, key, value.to_owned());
    }
    let mut moves = Vec::new();
    let mut to_play = "B";
    let mut recognized = false;
    let mut initialized = false;
    let mut warnings = decoded.warnings;
    let mut offset = 0;
    for raw in decoded.text.split_inclusive('\n') {
        let line = raw.trim();
        let position = offset + raw.len() - raw.trim_start().len();
        offset += raw.len();
        if let Some(header) = line.strip_prefix("\\[") {
            if let Some((key, value)) = header.strip_suffix("\\]").and_then(|s| s.split_once('=')) {
                recognized |= read_header(&mut root, key, value, position)?;
            }
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        match fields.first().copied() {
            Some("INI") => {
                if initialized || !moves.is_empty() {
                    return Err(error(position, "INI must occur once, before moves"));
                }
                initialized = true;
                recognized = true;
                let handicap = number(&fields, 3, position, "handicap")?;
                if handicap > 9 {
                    warnings.push(Warning {
                        position,
                        kind: WarningKind::GibHandicapClamped {
                            requested: handicap,
                        },
                    });
                }
                let handicap = handicap.min(9);
                put(&mut root, "HA", handicap.to_string());
                if handicap >= 2 {
                    let mut points = Vec::new();
                    let count = if handicap == 5 || handicap == 7 {
                        points.push(coordinate(9, 9));
                        handicap - 1
                    } else {
                        handicap
                    };
                    points.extend(
                        HANDICAP_POINTS
                            .iter()
                            .take(count as usize)
                            .map(|&(x, y)| coordinate(x, y)),
                    );
                    root.properties.push(Property {
                        identifier: "AB".into(),
                        values: points,
                    });
                    put(&mut root, "PL", "W".into());
                    to_play = "W";
                }
            }
            Some("STO") => {
                recognized = true;
                let color = fields
                    .get(3)
                    .ok_or_else(|| error(position, "missing STO color"))?;
                // Java explicitly treats every non-1 color code as white.
                let color = if *color == "1" { "B" } else { "W" };
                let x = number(&fields, 4, position, "x coordinate")?;
                let y = number(&fields, 5, position, "y coordinate")?;
                if x >= 19 || y >= 19 {
                    return Err(error(position, "STO coordinate outside 19x19 board"));
                }
                moves.push(move_node(color, coordinate(x as u8, y as u8)));
                to_play = opposite(color);
            }
            Some("SKI") => {
                recognized = true;
                moves.push(move_node(to_play, String::new()));
                to_play = opposite(to_play);
            }
            _ => {}
        }
    }
    if !recognized {
        return Err(error(0, "no GIB headers or records"));
    }
    if get(&root, "KM").is_none() {
        put(&mut root, "KM", "7.5".into());
    }
    let mut sequence = vec![root];
    sequence.extend(moves);
    Ok(Collection {
        games: vec![GameTree {
            sequence,
            children: vec![],
        }],
        encoding: decoded.encoding,
        warnings,
    })
}

fn read_header(root: &mut Node, key: &str, value: &str, position: usize) -> Result<bool, SgfError> {
    match key {
        "GAMEBLACKNAME" | "GAMEWHITENAME" => {
            let (name, rank) = player(value);
            let (name_key, rank_key) = if key == "GAMEBLACKNAME" {
                ("PB", "BR")
            } else {
                ("PW", "WR")
            };
            put(root, name_key, name.into());
            if let Some(rank) = rank {
                put(root, rank_key, rank.into());
            }
        }
        "GAMEINFOMAIN" => {
            if let Some(komi) = tagged(value, "GONGJE:") {
                put(root, "KM", tenths(komi, position, "GONGJE")?);
            }
            if let Some(result) = result(tagged(value, "GRLT:"), tagged(value, "ZIPSU:")) {
                put(root, "RE", result);
            }
        }
        "GAMETAG" => {
            if let Some(date) = tagged(value, "C").and_then(date) {
                put(root, "DT", date);
            }
            if get(root, "KM").is_none()
                && let Some(komi) = tagged(value, "G")
            {
                put(root, "KM", tenths(komi, position, "GAMETAG G")?);
            }
            if get(root, "RE").is_none()
                && let Some(result) = result(tagged(value, "W"), tagged(value, "Z"))
            {
                put(root, "RE", result);
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn player(value: &str) -> (&str, Option<&str>) {
    if let Some((name, rank)) = value.split_once('(')
        && !name.trim().is_empty()
        && !rank.contains('(')
        && let Some(rank) = rank.strip_suffix(')')
        && !rank.contains(')')
    {
        (name.trim(), Some(rank))
    } else {
        (value, None)
    }
}

fn tagged<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .split(',')
        .find_map(|part| part.trim().strip_prefix(prefix))
}

fn date(value: &str) -> Option<String> {
    // GAMETAG may continue with a time. Only the date's fixed-width prefix is read.
    let bytes = value.as_bytes();
    if bytes.len() < 10
        || bytes[4] != b':'
        || bytes[7] != b':'
        || !bytes[..4]
            .iter()
            .chain(&bytes[5..7])
            .chain(&bytes[8..10])
            .all(u8::is_ascii_digit)
    {
        return None;
    }
    Some(format!(
        "{}-{}-{}",
        &value[..4],
        &value[5..7],
        &value[8..10]
    ))
}

fn result(kind: Option<&str>, margin: Option<&str>) -> Option<String> {
    match kind? {
        "3" => Some("B+R".into()),
        "4" => Some("W+R".into()),
        "7" => Some("B+T".into()),
        "8" => Some("W+T".into()),
        kind @ ("0" | "1") => {
            let margin: u32 = margin?.parse().ok()?;
            let winner = if kind == "0" { 'B' } else { 'W' };
            Some(format!("{winner}+{}", f64::from(margin) / 10.0))
        }
        _ => None,
    }
}

fn tenths(value: &str, position: usize, field: &str) -> Result<String, SgfError> {
    let value: i32 = value
        .parse()
        .map_err(|_| error(position, format!("invalid {field}")))?;
    Ok((f64::from(value) / 10.0).to_string())
}

fn number(fields: &[&str], index: usize, position: usize, field: &str) -> Result<u32, SgfError> {
    fields
        .get(index)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| error(position, format!("invalid or missing {field}")))
}

fn coordinate(x: u8, y: u8) -> String {
    [char::from(b'a' + x), char::from(b'a' + y)]
        .into_iter()
        .collect()
}

fn opposite(color: &str) -> &'static str {
    if color == "B" { "W" } else { "B" }
}

fn move_node(color: &str, value: String) -> Node {
    let mut node = Node::default();
    put(&mut node, color, value);
    node
}

fn put(node: &mut Node, key: &str, value: String) {
    if let Some(property) = node.properties.iter_mut().find(|p| p.identifier == key) {
        property.values = vec![value];
    } else {
        node.properties.push(Property {
            identifier: key.into(),
            values: vec![value],
        });
    }
}

fn get<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    node.properties
        .iter()
        .find(|p| p.identifier == key)?
        .values
        .first()
        .map(String::as_str)
}

fn error(position: usize, reason: impl Into<String>) -> SgfError {
    SgfError {
        position,
        kind: ErrorKind::InvalidGib {
            reason: reason.into(),
        },
    }
}

#[cfg(test)]
mod tests;
