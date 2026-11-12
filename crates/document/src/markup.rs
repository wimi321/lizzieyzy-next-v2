//! Markup remains SGF data on the current node, independent of move legality.
use go_core::{BoardSize, Point};
use sgf::Property;

use crate::{DocError, GameDocument, NodeId, metadata, semantics};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkupKind {
    Triangle,
    Square,
    Circle,
    Cross,
    Letter,
    Number,
    Erase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Markup {
    pub point: Point,
    pub kind: MarkupKind,
    /// Labels preserve the complete imported text, not just ASCII letters.
    pub text: Option<String>,
}

impl GameDocument {
    pub fn markup_at(&self, id: NodeId) -> Result<Vec<Markup>, DocError> {
        let node = self.node(id)?;
        let size = self.markup_size()?;
        let mut marks = Vec::new();
        for property in node.properties() {
            let Some(kind) = property_kind(&property.identifier) else {
                continue;
            };
            for value in &property.values {
                if kind == MarkupKind::Letter {
                    if let Some((coord, text)) = value.split_once(':')
                        && let Ok(point) = Point::from_sgf(coord, size)
                    {
                        marks.push(Markup {
                            point,
                            kind: if parse_java_number(text).is_some() {
                                MarkupKind::Number
                            } else {
                                MarkupKind::Letter
                            },
                            text: Some(text.into()),
                        });
                    }
                } else if let Ok(points) =
                    semantics::setup_points(id, &property.identifier, value, size)
                {
                    marks.extend(points.into_iter().map(|point| Markup {
                        point,
                        kind,
                        text: None,
                    }));
                }
            }
        }
        Ok(marks)
    }

    pub(crate) fn updated_markup(
        &self,
        point: Point,
        kind: MarkupKind,
    ) -> Result<Vec<Property>, DocError> {
        let size = self.markup_size()?;
        let coordinate = point.to_sgf(size).map_err(|_| DocError::InvalidProperty {
            node: self.current,
            property: "markup".into(),
            value: format!("{point:?}"),
        })?;
        let properties = self.node(self.current)?.properties();
        let occupied = self
            .markup_at(self.current)?
            .iter()
            .any(|mark| mark.point == point);
        // Java tryToMarkup removes an existing mark regardless of the selected
        // tool. Erasing an empty point is a no-op. Never touch setup properties.
        if occupied || kind == MarkupKind::Erase {
            return Ok(remove_at(properties, self.current, point, size));
        }
        let (name, value) = match kind {
            MarkupKind::Triangle => ("TR", coordinate),
            MarkupKind::Square => ("SQ", coordinate),
            MarkupKind::Circle => ("CR", coordinate),
            MarkupKind::Cross => ("MA", coordinate),
            MarkupKind::Letter => ("LB", format!("{coordinate}:{}", next_letter(properties)?)),
            MarkupKind::Number => ("LB", format!("{coordinate}:{}", next_number(properties)?)),
            MarkupKind::Erase => return Ok(properties.to_vec()),
        };
        let mut updated = properties.to_vec();
        if let Some(property) = updated.iter_mut().find(|p| p.identifier == name) {
            property.values.push(value);
        } else {
            updated.push(Property {
                identifier: name.into(),
                values: vec![value],
            });
        }
        Ok(updated)
    }

    pub(crate) fn cleared_markup(&self) -> Result<Vec<Property>, DocError> {
        Ok(self
            .node(self.current)?
            .properties()
            .iter()
            .filter(|p| property_kind(&p.identifier).is_none())
            .cloned()
            .collect())
    }

    fn markup_size(&self) -> Result<BoardSize, DocError> {
        Ok(metadata::root_metadata(self.root, self.node(self.root)?.properties()).size)
    }
}

fn property_kind(identifier: &str) -> Option<MarkupKind> {
    Some(match identifier {
        "TR" => MarkupKind::Triangle,
        "SQ" => MarkupKind::Square,
        "CR" => MarkupKind::Circle,
        "MA" => MarkupKind::Cross,
        "LB" => MarkupKind::Letter,
        _ => return None,
    })
}

fn remove_at(
    properties: &[Property],
    node: NodeId,
    point: Point,
    size: BoardSize,
) -> Vec<Property> {
    let mut updated = Vec::new();
    for property in properties {
        let Some(kind) = property_kind(&property.identifier) else {
            updated.push(property.clone());
            continue;
        };
        let mut values = Vec::new();
        for value in &property.values {
            if kind == MarkupKind::Letter {
                let at_point = value
                    .split_once(':')
                    .and_then(|(coord, _)| Point::from_sgf(coord, size).ok())
                    == Some(point);
                if !at_point {
                    values.push(value.clone());
                }
            } else if let Ok(points) =
                semantics::setup_points(node, &property.identifier, value, size)
                && points.contains(&point)
            {
                // Only a compressed range containing this point is expanded;
                // all untouched values and properties retain their spelling.
                values.extend(
                    points
                        .into_iter()
                        .filter(|p| *p != point)
                        .filter_map(|p| p.to_sgf(size).ok()),
                );
            } else {
                values.push(value.clone());
            }
        }
        if !values.is_empty() {
            updated.push(Property {
                identifier: property.identifier.clone(),
                values,
            });
        } else if property.values.is_empty() {
            updated.push(property.clone());
        }
    }
    updated
}

fn labels(properties: &[Property]) -> impl Iterator<Item = &str> {
    properties
        .iter()
        .filter(|p| p.identifier == "LB")
        .flat_map(|p| &p.values)
        .filter_map(|value| {
            value
                .split_once(':')
                .map(|(_, text)| text.split(':').next().unwrap_or(""))
        })
}

fn next_letter(properties: &[Property]) -> Result<char, DocError> {
    // LizzieFrame.tryToMarkup compares the FIRST UTF-16 character of every LB,
    // takes max + 1, and skips ASCII punctuation between Z and a. It does not
    // fill gaps or implement spreadsheet-style AA labels.
    let highest = labels(properties)
        .filter_map(|text| text.encode_utf16().next())
        .max()
        .unwrap_or(u16::from(b'@'))
        .max(u16::from(b'@'));
    let mut next = u32::from(highest) + 1;
    if (91..=96).contains(&next) {
        next = 97;
    }
    char::from_u32(next)
        .filter(|_| next <= u32::from(u16::MAX))
        .ok_or_else(|| DocError::MarkupLabelLimit {
            kind: "letter".into(),
        })
}

// Java Integer.parseInt uses Character.digit on UTF-16 code units. In
// particular it accepts BMP decimal digits but rejects supplementary digits.
// These are the ten-code-unit decimal blocks verified against Java 17.
fn parse_java_number(text: &str) -> Option<i32> {
    const DECIMAL_ZEROES: [u16; 37] = [
        0x0030, 0x0660, 0x06f0, 0x07c0, 0x0966, 0x09e6, 0x0a66, 0x0ae6, 0x0b66, 0x0be6, 0x0c66,
        0x0ce6, 0x0d66, 0x0de6, 0x0e50, 0x0ed0, 0x0f20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946,
        0x19d0, 0x1a80, 0x1a90, 0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0,
        0xa9f0, 0xaa50, 0xabf0, 0xff10,
    ];
    let (negative, digits) = if let Some(digits) = text.strip_prefix('-') {
        (true, digits)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    if digits.is_empty() {
        return None;
    }
    let mut number = 0i64;
    for unit in digits.encode_utf16() {
        let digit = DECIMAL_ZEROES
            .iter()
            .find_map(|zero| unit.checked_sub(*zero).filter(|digit| *digit < 10))?;
        number = number.checked_mul(10)?.checked_add(i64::from(digit))?;
        if number > i64::from(i32::MAX) + i64::from(negative) {
            return None;
        }
    }
    i32::try_from(if negative { -number } else { number }).ok()
}

fn next_number(properties: &[Property]) -> Result<i32, DocError> {
    labels(properties)
        .filter_map(parse_java_number)
        .max()
        .unwrap_or(0)
        .max(0)
        .checked_add(1)
        .ok_or_else(|| DocError::MarkupLabelLimit {
            kind: "number".into(),
        })
}

#[cfg(test)]
mod tests;
