use go_core::{BoardSize, Color, RulePreset};
use sgf::Property;

use crate::{DocumentWarning, NodeId, WarningKind};

const DEFAULT_SIZE: BoardSize = BoardSize {
    width: 19,
    height: 19,
};
const DEFAULT_KOMI: f64 = 7.5;

pub(crate) struct RootMetadata {
    pub size: BoardSize,
    pub rules: RulePreset,
    pub rules_raw: Option<String>,
    pub handicap: u32,
    pub komi: f64,
    pub warnings: Vec<DocumentWarning>,
}

/// The same recovery path supplies replay values and document diagnostics.
pub(crate) fn root_metadata(node: NodeId, properties: &[Property]) -> RootMetadata {
    let mut warnings = Vec::new();
    let size = read_size(node, properties, &mut warnings);
    let rules_raw = first_value(node, properties, "RU", &mut warnings).map(str::to_owned);
    let rules = match rules_raw.as_deref() {
        None => RulePreset::Chinese,
        Some(value) => match RulePreset::from_name(&value.trim().to_ascii_lowercase()) {
            Ok(rules) => rules,
            Err(_) => {
                warn(
                    &mut warnings,
                    node,
                    "RU",
                    WarningKind::UnknownRules {
                        fallback: "chinese".into(),
                    },
                );
                RulePreset::Chinese
            }
        },
    };
    let handicap = match first_value(node, properties, "HA", &mut warnings) {
        None => 0,
        Some(value) => match value.trim().parse() {
            Ok(handicap) => handicap,
            Err(_) => {
                warn(
                    &mut warnings,
                    node,
                    "HA",
                    WarningKind::InvalidValue {
                        fallback: "0".into(),
                    },
                );
                0
            }
        },
    };
    let komi = read_komi(node, properties, size, &mut warnings);
    RootMetadata {
        size,
        rules,
        rules_raw,
        handicap,
        komi,
        warnings,
    }
}

pub(crate) fn player(
    node: NodeId,
    properties: &[Property],
    warnings: &mut Vec<DocumentWarning>,
) -> Option<Color> {
    let value = first_value(node, properties, "PL", warnings)?;
    let player = if value.eq_ignore_ascii_case("B") {
        Color::Black
    } else if value.eq_ignore_ascii_case("W") {
        Color::White
    } else {
        warn(warnings, node, "PL", WarningKind::IgnoredPlayer);
        return None;
    };
    if !matches!(value, "B" | "W") {
        warn(warnings, node, "PL", WarningKind::NormalizedCase);
    }
    Some(player)
}

pub(crate) fn non_root_warnings(
    node: NodeId,
    properties: &[Property],
    warnings: &mut Vec<DocumentWarning>,
) {
    for name in ["SZ", "KM", "RU", "HA"] {
        if properties
            .iter()
            .any(|property| property.identifier == name)
        {
            warn(warnings, node, name, WarningKind::IgnoredNonRoot);
        }
    }
}

fn read_size(
    node: NodeId,
    properties: &[Property],
    warnings: &mut Vec<DocumentWarning>,
) -> BoardSize {
    let Some(value) = first_value(node, properties, "SZ", warnings) else {
        return DEFAULT_SIZE;
    };
    let (width, height) = value.split_once(':').unwrap_or((value, value));
    let parsed = width
        .trim()
        .parse::<u32>()
        .and_then(|width| height.trim().parse::<u32>().map(|height| (width, height)));
    let kind = match parsed {
        Ok((width, height)) if (2..=25).contains(&width) && (2..=25).contains(&height) => {
            return BoardSize {
                width: width as u8,
                height: height as u8,
            };
        }
        Ok(_) => WarningKind::OutOfRange {
            fallback: "19x19".into(),
        },
        Err(_) => WarningKind::InvalidValue {
            fallback: "19x19".into(),
        },
    };
    warn(warnings, node, "SZ", kind);
    DEFAULT_SIZE
}

fn read_komi(
    node: NodeId,
    properties: &[Property],
    size: BoardSize,
    warnings: &mut Vec<DocumentWarning>,
) -> f64 {
    let km = first_value(node, properties, "KM", warnings);
    match km.map(|value| parsed_komi(value, properties, size)) {
        Some(Ok(komi)) => return komi,
        Some(Err(KomiFailure::OutOfRange)) => {
            warn(
                warnings,
                node,
                "KM",
                WarningKind::OutOfRange {
                    fallback: "7.5".into(),
                },
            );
            return DEFAULT_KOMI;
        }
        Some(Err(KomiFailure::Invalid)) | None => {}
    }
    let ko = first_value(node, properties, "KO", warnings)
        .map(|value| parsed_komi(value, properties, size));
    if km.is_some() {
        warn(
            warnings,
            node,
            "KM",
            WarningKind::InvalidValue {
                fallback: if matches!(ko, Some(Ok(_))) {
                    "KO"
                } else {
                    "7.5"
                }
                .into(),
            },
        );
    }
    match ko {
        Some(Ok(komi)) => {
            warn(warnings, node, "KO", WarningKind::KomiFromKo);
            komi
        }
        Some(Err(reason)) => {
            let kind = match reason {
                KomiFailure::Invalid => WarningKind::InvalidValue {
                    fallback: "7.5".into(),
                },
                KomiFailure::OutOfRange => WarningKind::OutOfRange {
                    fallback: "7.5".into(),
                },
            };
            warn(warnings, node, "KO", kind);
            DEFAULT_KOMI
        }
        None => DEFAULT_KOMI,
    }
}

#[derive(Clone, Copy)]
enum KomiFailure {
    Invalid,
    OutOfRange,
}

fn parsed_komi(value: &str, properties: &[Property], size: BoardSize) -> Result<f64, KomiFailure> {
    let raw = value
        .trim()
        .parse::<f64>()
        .map_err(|_| KomiFailure::Invalid)?;
    if !raw.is_finite() {
        return Err(KomiFailure::Invalid);
    }
    let komi = normalize_komi(raw, properties);
    if komi.abs() >= size.area() as f64 {
        return Err(KomiFailure::OutOfRange);
    }
    Ok(komi)
}

fn first_value<'a>(
    node: NodeId,
    properties: &'a [Property],
    name: &str,
    warnings: &mut Vec<DocumentWarning>,
) -> Option<&'a str> {
    let mut matching = properties
        .iter()
        .filter(|property| property.identifier == name);
    let first = matching.next()?;
    if first.values.len() > 1 || matching.next().is_some() {
        warn(warnings, node, name, WarningKind::FirstValueUsed);
    }
    // Programmatic SGF nodes may lack a value even though the parser rejects
    // that syntax. Treat the missing first value like an empty SGF value.
    Some(first.values.first().map_or("", String::as_str))
}

fn warn(warnings: &mut Vec<DocumentWarning>, node: NodeId, property: &str, kind: WarningKind) {
    warnings.push(DocumentWarning {
        node,
        property: property.into(),
        kind,
    });
}

/// Match Java SGFParser's source detection and nested normalization conditions.
pub(crate) fn normalize_komi(mut komi: f64, properties: &[Property]) -> f64 {
    let contains = |name: &str, needle: &str| {
        properties
            .iter()
            .find(|p| p.identifier == name)
            .and_then(|p| p.values.first())
            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
    };
    if contains("AP", "foxwq") {
        if komi.abs() >= 50.0 {
            komi /= 100.0;
            if contains("RU", "chinese") && (-4.0..=4.0).contains(&komi) {
                komi *= 2.0;
            }
        }
    } else {
        if komi >= 200.0 {
            komi /= 100.0;
            if (-4.0..=4.0).contains(&komi) {
                komi *= 2.0;
            }
        }
        // Java prints magnitudes >= 10^7 in exponent notation, without this suffix.
        if komi.abs() < 10_000_000.0 && matches!(komi.fract().abs(), 0.25 | 0.75) {
            komi *= 2.0;
        }
    }
    komi
}

#[cfg(test)]
mod tests;
