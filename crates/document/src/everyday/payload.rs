//! Transform only coordinate tokens, retaining Java payload precision and
//! unknown numeric extensions verbatim instead of parse/format normalization.
use go_core::{BoardSize, Color, Move, Point};

use super::BoardTransform;

pub(super) fn transform(
    value: &str,
    size: BoardSize,
    transform: BoardTransform,
) -> Result<String, ()> {
    crate::lz::parse(value, size, Color::Black).map_err(|_| ())?;
    let Some((_, body)) = value.split_once('\n') else {
        return Ok(value.into());
    };
    let offset = value.len() - body.len();
    let mut spans = Vec::new();
    let mut cursor = offset;
    for token in body.split_whitespace() {
        let start = cursor + value[cursor..].find(token).ok_or(())?;
        spans.push((start, start + token.len()));
        cursor = start + token.len();
    }
    let tokens: Vec<_> = spans
        .iter()
        .map(|&(start, end)| &value[start..end])
        .collect();
    let mut changes = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index] {
            "info" => index += 1,
            "ownership" => {
                let ownership = &tokens[index + 1..];
                if ownership.len() != usize::from(size.width) * usize::from(size.height) {
                    return Err(());
                }
                for (from, token) in ownership.iter().enumerate() {
                    let point = transform.point(
                        Point {
                            x: (from % usize::from(size.width)) as u8,
                            y: (from / usize::from(size.width)) as u8,
                        },
                        size,
                    );
                    let to = usize::from(point.y) * usize::from(size.width) + usize::from(point.x);
                    changes.push((index + 1 + to, (*token).to_owned()));
                }
                break;
            }
            "pv" => {
                index += 1;
                while index < tokens.len()
                    && !matches!(tokens[index], "pvVisits" | "info" | "ownership")
                {
                    changes.push((index, coordinate(tokens[index], size, transform)?));
                    index += 1;
                }
            }
            "pvVisits" => {
                index += 1;
                while index < tokens.len() && !matches!(tokens[index], "info" | "ownership") {
                    index += 1;
                }
            }
            key => {
                if index + 1 >= tokens.len() {
                    return Err(());
                }
                if key == "move" {
                    changes.push((index + 1, coordinate(tokens[index + 1], size, transform)?));
                }
                index += 2;
            }
        }
    }
    changes.sort_by_key(|(index, _)| *index);
    let mut output = String::with_capacity(value.len());
    let mut copied = 0;
    for (index, token) in changes {
        let (start, end) = spans[index];
        output.push_str(&value[copied..start]);
        output.push_str(&token);
        copied = end;
    }
    output.push_str(&value[copied..]);
    Ok(output)
}

fn coordinate(value: &str, size: BoardSize, transform: BoardTransform) -> Result<String, ()> {
    match Move::from_gtp(Color::Black, value, size).map_err(|_| ())? {
        Move::Pass(_) => Ok(value.into()),
        Move::Play(_, point) => {
            let coordinate = transform.point(point, size).to_gtp(size).map_err(|_| ())?;
            Ok(
                if value.as_bytes().first().is_some_and(u8::is_ascii_lowercase) {
                    coordinate.to_ascii_lowercase()
                } else {
                    coordinate
                },
            )
        }
    }
}
