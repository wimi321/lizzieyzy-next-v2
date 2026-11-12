//! Java SGF analysis payloads, independent of engine transports.
//!
//! Java headers store the opponent's win percentage, while candidates and
//! score/ownership values use the player to move. Public values are Black's.
use std::collections::{BTreeMap, BTreeSet};

use go_core::{BoardSize, Color, Move};
use sgf::{GameTree, Property};

use crate::{DocError, DocumentWarning, GameDocument, NodeId, WarningKind, metadata};

mod import;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SavedAnalysis {
    pub engine_name: String,
    pub winrate: f64,
    pub visits: u64,
    pub score_lead: Option<f64>,
    pub score_stdev: Option<f64>,
    pub pda: Option<f64>,
    pub wrn: Option<f64>,
    pub candidates: Vec<SavedCandidate>,
    pub ownership: Option<Vec<f64>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SavedCandidate {
    pub coordinate: String,
    pub visits: u64,
    pub winrate: f64,
    pub score_lead: Option<f64>,
    pub score_stdev: Option<f64>,
    /// Probability, not Java's percentage policy value.
    pub prior: f64,
    pub order: u32,
    pub edge_visits: Option<u64>,
    pub pv: Vec<String>,
    pub pv_visits: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid saved analysis: {0}")]
pub struct LzError(pub String);

impl GameDocument {
    /// Reads the last valid primary slot; malformed payloads are diagnosed by
    /// warnings(), never prevent opening a game, and remain in the raw SGF.
    pub fn saved_analysis(&self, id: NodeId) -> Result<Option<SavedAnalysis>, DocError> {
        let node = self.node(id)?;
        if !node.properties.iter().any(|p| is_primary(&p.identifier)) {
            return Ok(None);
        }
        let position = self.position_at(id)?;
        Ok(import::at_position(node, &position))
    }

    /// Exports a detached tree. `analyses` is the complete set of currently
    /// valid primary results, so valid but stale imported slots are removed.
    /// Secondary slots and malformed raw data are preserved for other readers.
    pub fn to_sgf_with_analysis(
        &self,
        analyses: &BTreeMap<NodeId, SavedAnalysis>,
    ) -> Result<GameTree, LzError> {
        self.to_sgf_with_analysis_and_warnings(analyses)
            .map(|(tree, _)| tree)
    }

    /// Saturates calculation counts to Java's i32 limit without mutating the
    /// document or retained results. Each affected node yields one warning.
    pub fn to_sgf_with_analysis_and_warnings(
        &self,
        analyses: &BTreeMap<NodeId, SavedAnalysis>,
    ) -> Result<(GameTree, Vec<DocumentWarning>), LzError> {
        // Unchanged file results keep their exact property order, extensions
        // and original formatting, unless their counts need Java saturation.
        let unchanged: BTreeSet<_> = analyses
            .iter()
            .filter_map(|(&id, analysis)| {
                if counts_exceed_java_range(analysis) {
                    return None;
                }
                self.saved_analysis(id)
                    .ok()
                    .flatten()
                    .filter(|original| original == analysis)
                    .map(|_| id)
            })
            .collect();
        let mut copy = self.clone();
        let root = self
            .node(self.root)
            .map_err(|err| LzError(err.to_string()))?;
        let size = metadata::root_metadata(self.root, &root.properties).size;
        for node in copy.arena.iter_mut().filter(|node| node.live) {
            if unchanged.contains(&node.id) {
                continue;
            }
            for property in &mut node.properties {
                if is_primary(&property.identifier) {
                    property
                        .values
                        .retain(|value| parse(value, size, Color::Black).is_err());
                }
            }
            node.properties
                .retain(|p| !is_primary(&p.identifier) || !p.values.is_empty());
        }
        let mut warnings = Vec::new();
        for (&id, analysis) in analyses {
            if unchanged.contains(&id) {
                continue;
            }
            let position = self
                .position_at(id)
                .map_err(|err| LzError(err.to_string()))?;
            let (payload, clamped) =
                format_with_warnings(analysis, position.board.size(), position.board.to_play())?;
            if clamped {
                warnings.push(DocumentWarning {
                    node: id,
                    property: "LZ".into(),
                    kind: WarningKind::OutOfRange {
                        fallback: JAVA_MAX_VISITS.to_string(),
                    },
                });
            }
            if let Some(node) = copy.arena.get_mut(id.0 as usize).filter(|n| n.live) {
                node.properties.push(Property {
                    identifier: "LZ".into(),
                    values: vec![payload],
                });
            }
        }
        Ok((copy.to_sgf(), warnings))
    }

    pub(crate) fn analysis_warnings(&self) -> Vec<DocumentWarning> {
        let size =
            metadata::root_metadata(self.root, &self.arena[self.root.0 as usize].properties).size;
        self.arena
            .iter()
            .filter(|node| node.live)
            .flat_map(|node| {
                node.properties
                    .iter()
                    .filter(|p| is_analysis(&p.identifier))
                    .flat_map(move |property| {
                        property
                            .values
                            .iter()
                            .filter(move |value| parse(value, size, Color::Black).is_err())
                            .map(move |_| DocumentWarning {
                                node: node.id,
                                property: property.identifier.clone(),
                                kind: WarningKind::InvalidValue {
                                    fallback: "ignored".into(),
                                },
                            })
                    })
            })
            .collect()
    }
}

fn is_primary(name: &str) -> bool {
    matches!(name, "LZ" | "LZOP")
}
fn is_analysis(name: &str) -> bool {
    matches!(name, "LZ" | "LZOP" | "LZ2" | "LZOP2")
}
fn failure(field: &str) -> LzError {
    LzError(field.into())
}
fn finite(value: &str) -> Result<f64, LzError> {
    value
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| failure("non-finite or malformed number"))
}
fn probability(value: f64) -> Result<f64, LzError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(failure("winrate/prior outside 0..1"))
    }
}
fn visits(value: &str) -> Result<u64, LzError> {
    let normalized = value.to_ascii_lowercase().replace(',', "");
    let (number, scale) = if let Some(number) = normalized.strip_suffix('k') {
        (number, 1_000.0)
    } else if let Some(number) = normalized.strip_suffix('m') {
        (number, 1_000_000.0)
    } else {
        (normalized.as_str(), 1.0)
    };
    let count = finite(number)? * scale;
    if !(0.0..=9_007_199_254_740_991.0).contains(&count) {
        return Err(failure("visits outside exact integer range"));
    }
    Ok(count.round() as u64)
}
fn black_probability(value: f64, player: Color) -> f64 {
    if player == Color::Black {
        value
    } else {
        1.0 - value
    }
}
fn black_score(value: f64, player: Color) -> f64 {
    if player == Color::Black {
        value
    } else {
        -value
    }
}
fn optional(tokens: &[&str], index: usize) -> Result<Option<f64>, LzError> {
    tokens
        .get(index)
        .filter(|token| !token.contains('='))
        .map(|token| finite(token))
        .transpose()
}

/// Parses one LZ/LZOP/LZ2/LZOP2 value. Unknown named extensions are ignored.
/// Invalid WRN follows Java's unknown-value semantics rather than rejecting
/// otherwise valid analysis; malformed mandatory values reject this slot.
pub fn parse(value: &str, size: BoardSize, player: Color) -> Result<SavedAnalysis, LzError> {
    let (header, body) = value.split_once('\n').unwrap_or((value, ""));
    let tokens: Vec<_> = header.split_whitespace().collect();
    if tokens.len() < 3 {
        return Err(failure("header needs engine, winrate and visits"));
    }
    let mut analysis = SavedAnalysis {
        engine_name: tokens[0].into(),
        winrate: black_probability(1.0 - probability(finite(tokens[1])? / 100.0)?, player),
        visits: visits(tokens[2])?,
        score_lead: optional(&tokens, 3)?.map(|value| black_score(value, player)),
        score_stdev: optional(&tokens, 4)?,
        pda: optional(&tokens, 5)?,
        ..SavedAnalysis::default()
    };
    for token in tokens.iter().skip(6) {
        if let Some(value) = token.strip_prefix("rootVisits=") {
            analysis.visits = visits(value)?;
        }
        if let Some(value) = token.strip_prefix("wrn=") {
            analysis.wrn = finite(value).ok().filter(|v| (0.0..=2.0).contains(v));
        }
    }
    if analysis.score_stdev.is_some_and(|value| value < 0.0) {
        return Err(failure("negative score standard deviation"));
    }
    if analysis.visits == 0 {
        return Ok(analysis);
    }
    let tokens: Vec<_> = body.split_whitespace().collect();
    let ownership_index = tokens
        .iter()
        .position(|v| *v == "ownership")
        .unwrap_or(tokens.len());
    if ownership_index < tokens.len() {
        analysis.ownership = Some(
            tokens[ownership_index + 1..]
                .iter()
                .map(|value| {
                    let value = finite(value)?;
                    if !(-1.0..=1.0).contains(&value) {
                        return Err(failure("ownership outside -1..1"));
                    }
                    Ok(black_score(value, player))
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    for candidate in tokens[..ownership_index]
        .split(|token| *token == "info")
        .filter(|tokens| !tokens.is_empty())
    {
        analysis.candidates.push(parse_candidate(
            candidate,
            size,
            player,
            analysis.candidates.len() as u32,
        )?);
    }
    // Java's short legacy header obtains score from its first candidate.
    if analysis.score_lead.is_none() {
        analysis.score_lead = analysis.candidates.first().and_then(|c| c.score_lead);
    }
    if analysis.score_stdev.is_none() {
        analysis.score_stdev = analysis.candidates.first().and_then(|c| c.score_stdev);
    }
    Ok(analysis)
}

fn parse_candidate(
    tokens: &[&str],
    size: BoardSize,
    player: Color,
    order: u32,
) -> Result<SavedCandidate, LzError> {
    let mut result = SavedCandidate {
        order,
        ..SavedCandidate::default()
    };
    let mut has_visits = false;
    let mut has_winrate = false;
    let mut index = 0;
    while index < tokens.len() {
        let key = tokens[index];
        index += 1;
        if key == "pv" {
            while index < tokens.len() && tokens[index] != "pvVisits" {
                Move::from_gtp(player, tokens[index], size)
                    .map_err(|_| failure("invalid PV coordinate"))?;
                result.pv.push(tokens[index].to_owned());
                index += 1;
            }
            if index < tokens.len() {
                result.pv_visits = tokens[index + 1..]
                    .iter()
                    .map(|value| visits(value))
                    .collect::<Result<Vec<_>, _>>()?;
            }
            break;
        }
        let value = tokens
            .get(index)
            .ok_or_else(|| failure("candidate field missing value"))?;
        index += 1;
        match key {
            "move" => {
                Move::from_gtp(player, value, size)
                    .map_err(|_| failure("invalid candidate coordinate"))?;
                result.coordinate = (*value).into();
            }
            "visits" => {
                result.visits = visits(value)?;
                has_visits = true;
            }
            "winrate" => {
                result.winrate = black_probability(probability(finite(value)? / 10_000.0)?, player);
                has_winrate = true;
            }
            "scoreMean" => result.score_lead = Some(black_score(finite(value)?, player)),
            "scoreStdev" => result.score_stdev = Some(finite(value)?),
            "prior" => {
                // Integer prior is basis points; decimal prior is percentage,
                // matching SGFParser.parsePriorValue rather than guessing.
                let scale = if value.parse::<i32>().is_ok() {
                    10_000.0
                } else {
                    100.0
                };
                result.prior = probability(finite(value)? / scale)?;
            }
            "order" => {
                result.order = value
                    .parse()
                    .map_err(|_| failure("invalid candidate order"))?
            }
            "edgeVisits" => result.edge_visits = Some(visits(value)?),
            _ => {}
        }
    }
    if result.coordinate.is_empty() || !has_visits || !has_winrate {
        return Err(failure("incomplete candidate"));
    }
    if result.pv.is_empty() {
        result.pv.push(result.coordinate.clone());
    }
    Ok(result)
}

const JAVA_MAX_VISITS: u64 = i32::MAX as u64;

fn counts_exceed_java_range(analysis: &SavedAnalysis) -> bool {
    analysis.visits > JAVA_MAX_VISITS
        || analysis.candidates.iter().any(|candidate| {
            candidate.visits > JAVA_MAX_VISITS
                || candidate
                    .edge_visits
                    .iter()
                    .chain(&candidate.pv_visits)
                    .any(|&count| count > JAVA_MAX_VISITS)
        })
}

/// Emits Java's six legacy header slots and named rootVisits/WRN extensions.
/// Candidate winrates have Java's 1/100 percentage-point precision.
pub fn format(analysis: &SavedAnalysis, size: BoardSize, player: Color) -> Result<String, LzError> {
    format_with_warnings(analysis, size, player).map(|(payload, _)| payload)
}

/// The boolean records whether any calculation count was saturated. Callers
/// exporting a document attach the diagnostic to that node, once per payload.
pub fn format_with_warnings(
    analysis: &SavedAnalysis,
    size: BoardSize,
    player: Color,
) -> Result<(String, bool), LzError> {
    probability(analysis.winrate)?;
    let clamped = counts_exceed_java_range(analysis);
    let root_visits = analysis.visits.min(JAVA_MAX_VISITS);
    for value in [analysis.score_lead, analysis.score_stdev, analysis.pda]
        .into_iter()
        .flatten()
    {
        if !value.is_finite() {
            return Err(failure("non-finite analysis scalar"));
        }
    }
    if analysis
        .wrn
        .is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value))
    {
        return Err(failure("WRN outside 0..2"));
    }
    if analysis.engine_name.is_empty() || analysis.engine_name.chars().any(char::is_whitespace) {
        return Err(failure("engine name must be a single nonempty token"));
    }
    let mut output = format!(
        "{} {} {}",
        analysis.engine_name,
        100.0 * (1.0 - black_probability(analysis.winrate, player)),
        root_visits
    );
    let include_pda = analysis.pda.is_some() || analysis.wrn.is_some();
    let include_stdev = analysis.score_stdev.is_some() || include_pda;
    if analysis.score_lead.is_some() || include_stdev {
        output.push_str(&format!(
            " {}",
            black_score(analysis.score_lead.unwrap_or(0.0), player)
        ));
        if include_stdev {
            output.push_str(&format!(" {}", analysis.score_stdev.unwrap_or(0.0)));
        }
        if include_pda {
            output.push_str(&format!(
                " {} rootVisits={}",
                analysis.pda.unwrap_or(0.0),
                root_visits
            ));
        }
        if let Some(wrn) = analysis.wrn {
            output.push_str(&format!(" wrn={wrn}"));
        }
    }
    let mut candidates = Vec::new();
    for candidate in &analysis.candidates {
        if candidate.order > i32::MAX as u32 {
            return Err(failure(
                "candidate order exceeds Java's signed 32-bit range",
            ));
        }
        probability(candidate.winrate)?;
        probability(candidate.prior)?;
        for value in [candidate.score_lead, candidate.score_stdev]
            .into_iter()
            .flatten()
        {
            if !value.is_finite() {
                return Err(failure("non-finite candidate scalar"));
            }
        }
        for coordinate in std::iter::once(&candidate.coordinate).chain(&candidate.pv) {
            Move::from_gtp(player, coordinate, size)
                .map_err(|_| failure("invalid candidate/PV coordinate"))?;
        }
        let mut value = format!(
            "move {} visits {} winrate {} prior {:.10}",
            candidate.coordinate,
            candidate.visits.min(JAVA_MAX_VISITS),
            (black_probability(candidate.winrate, player) * 10_000.0).round() as u64,
            candidate.prior * 100.0
        );
        if let Some(score) = candidate.score_lead {
            value.push_str(&format!(" scoreMean {}", black_score(score, player)));
        }
        if let Some(stdev) = candidate.score_stdev {
            value.push_str(&format!(" scoreStdev {stdev}"));
        }
        value.push_str(&format!(" order {}", candidate.order));
        if let Some(visits) = candidate.edge_visits {
            value.push_str(&format!(" edgeVisits {}", visits.min(JAVA_MAX_VISITS)));
        }
        value.push_str(" pv ");
        value.push_str(&if candidate.pv.is_empty() {
            candidate.coordinate.clone()
        } else {
            candidate.pv.join(" ")
        });
        if !candidate.pv_visits.is_empty() {
            value.push_str(" pvVisits ");
            value.push_str(
                &candidate
                    .pv_visits
                    .iter()
                    .map(|&count| count.min(JAVA_MAX_VISITS).to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        candidates.push(value);
    }
    if !candidates.is_empty() || analysis.ownership.is_some() {
        output.push('\n');
        output.push_str(&candidates.join(" info "));
        if let Some(ownership) = &analysis.ownership {
            output.push_str(" ownership");
            for value in ownership {
                if !value.is_finite() || !(-1.0..=1.0).contains(value) {
                    return Err(failure("ownership outside -1..1"));
                }
                output.push_str(&format!(" {}", black_score(*value, player)));
            }
        }
    }
    // Validate outbound coordinates/numbers through the same parser before a
    // save can write a corrupt slot. No document or history is changed.
    parse(&output, size, player)?;
    Ok((output, clamped))
}

#[cfg(test)]
mod tests;
