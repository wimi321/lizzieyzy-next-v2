//! Offline display names matching Java's weight-name rules. Display metadata
//! never identifies a model for execution or changes its cache fingerprint.

use crate::{EngineProfile, engine_setup::catalog::ModelCatalog};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum EngineDisplayName {
    Text(String),
    Catalog(String),
}

/// Prefer the already recognized model name; otherwise use the model basename.
/// This query neither reads the model nor launches an engine.
pub fn engine_display_name(profile: &EngineProfile) -> EngineDisplayName {
    let path = profile.model.to_string_lossy();
    let name = profile
        .canonical_model_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&path);
    display_name(name, &ModelCatalog::bundled())
}

fn display_name(name: &str, catalog: &ModelCatalog) -> EngineDisplayName {
    let base = strip_extension(name.trim().rsplit(['/', '\\']).next().unwrap_or_default());
    if base.is_empty() {
        return EngineDisplayName::Text("KataGo".into());
    }
    for (id, model) in &catalog.models {
        if !base.eq_ignore_ascii_case(strip_extension(&model.file_name)) {
            continue;
        }
        match id.as_str() {
            "b10-lightweight" | "b10-balanced" => return EngineDisplayName::Catalog(id.clone()),
            "b11-flagship" => {
                if let Some(date) = model
                    .published_at
                    .as_deref()
                    .filter(|date| !date.is_empty())
                {
                    return EngineDisplayName::Text(format!("Transformer B11 · {date}"));
                }
            }
            _ => {}
        }
    }
    if base.eq_ignore_ascii_case("b11c768h12nbt3tflrs-fson-silu") {
        // Catalog ids also select Java's localized tier names for older models.
        return EngineDisplayName::Catalog("b11-flagship".into());
    }
    let text = if base.eq_ignore_ascii_case("kata1-b28c512nbt-s13255194368-d5935380940") {
        "28B 2026-06".into()
    } else if is_transformer(base) {
        trained_transformer(base)
            .map(|(blocks, suffix)| format!("B{blocks} · {suffix}"))
            .unwrap_or_else(|| base.into())
    } else {
        classic_name(base).unwrap_or_else(|| base.into())
    };
    EngineDisplayName::Text(text)
}

fn strip_extension(name: &str) -> &str {
    let name = name.trim();
    let lower = name.to_ascii_lowercase();
    for extension in [".bin.gz", ".txt.gz", ".bin", ".txt", ".gz"] {
        if lower.ends_with(extension) {
            return &name[..name.len() - extension.len()];
        }
    }
    name
}

// Match Java's b<digits>c<digits> prefix, preserving the original suffix.
fn family(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix(['b', 'B'])?;
    let blocks_end = rest.bytes().take_while(u8::is_ascii_digit).count();
    if blocks_end == 0 {
        return None;
    }
    let channels = rest[blocks_end..].strip_prefix(['c', 'C'])?;
    let channels_end = channels.bytes().take_while(u8::is_ascii_digit).count();
    (channels_end > 0).then(|| (&rest[..blocks_end], &channels[channels_end..]))
}

fn is_transformer(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("tflrs")
        || ["kata1-tf2-", "kata1-tf3-"].iter().any(|prefix| {
            lower
                .strip_prefix(prefix)
                .and_then(family)
                .is_some_and(|(_, suffix)| suffix.starts_with('-'))
        })
}

fn trained_transformer(name: &str) -> Option<(&str, &str)> {
    let lower = name.to_ascii_lowercase();
    let version = lower.strip_prefix("kata1-tf")?;
    let version_len = version.bytes().take_while(u8::is_ascii_digit).count();
    if version_len == 0 || !version[version_len..].starts_with('-') {
        return None;
    }
    let (blocks, tail) = family(&name["kata1-tf".len() + version_len + 1..])?;
    let suffix = tail.strip_prefix('-')?;
    (!suffix.is_empty() && !suffix.contains(['\r', '\n'])).then_some((blocks, suffix))
}

fn classic_name(name: &str) -> Option<String> {
    if !name.to_ascii_lowercase().starts_with("kata1-") {
        return None;
    }
    let rest = &name["kata1-".len()..];
    // The Java alias group is greedy: prefer the rightmost valid family token.
    let mut starts = vec![0];
    starts.extend(rest.match_indices('-').map(|(index, _)| index + 1));
    for start in starts.into_iter().rev() {
        let segment = rest[start..].split('-').next()?;
        let Some((blocks, _)) = family(segment) else {
            continue;
        };
        let alias = if start == 0 { "" } else { &rest[..start - 1] };
        if !alias.is_empty() && !valid_alias(alias) {
            continue;
        }
        let tail = &rest[start + segment.len()..];
        if tail == "-" || tail.contains(['\r', '\n']) {
            continue;
        }
        let mut parts = Vec::new();
        if !alias.is_empty() {
            parts.push(alias.to_owned());
        }
        parts.push(format!("{blocks}B"));
        parts.extend(
            tail.split(|ch: char| ch == '-' || ch.is_ascii_whitespace())
                .filter(|token| meaningful_suffix(token))
                .take(2)
                .map(str::to_owned),
        );
        return Some(parts.join(" "));
    }
    None
}

fn valid_alias(alias: &str) -> bool {
    alias
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphabetic)
        && alias
            .split('-')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric()))
}

fn meaningful_suffix(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    if !bytes.iter().any(u8::is_ascii_alphabetic) {
        return false;
    }
    if bytes.len() > 1
        && matches!(bytes[0], b's' | b'd')
        && bytes[1..].iter().all(u8::is_ascii_hexdigit)
    {
        return false;
    }
    // Java hides short letter prefixes followed by a long hexadecimal/hash id.
    !(1..=3).any(|prefix| {
        bytes.len() >= prefix + 4
            && bytes[..prefix].iter().all(u8::is_ascii_alphabetic)
            && bytes[prefix..]
                .iter()
                .all(|byte| byte.is_ascii_hexdigit() || *byte == b'x')
    })
}

#[cfg(test)]
mod tests;
