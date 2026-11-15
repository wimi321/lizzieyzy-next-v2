//! KataGo GTP analysis reports. Values are probabilities and side-to-play scores;
//! converting a report's perspective belongs to the caller, not this codec.
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum AnalyzeError {
    #[error("analysis line exceeds size limit")]
    TooLarge,
    #[error("unexpected analysis token")]
    UnexpectedToken,
    #[error("missing analysis field: {0}")]
    Missing(&'static str),
    #[error("invalid analysis field: {0}")]
    Invalid(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub coordinate: String,
    pub visits: u64,
    pub winrate: f64,
    pub score_lead: f64,
    pub prior: f64,
    pub order: u32,
    pub pv: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Root {
    pub visits: u64,
    pub winrate: f64,
    pub score_lead: f64,
    pub utility: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub moves: Vec<Candidate>,
    pub root: Option<Root>,
    pub ownership: Option<Vec<f64>>,
}

fn boundary(token: &str) -> bool {
    matches!(token, "info" | "rootInfo" | "ownership" | "ownershipStdev")
}

fn field<'a>(fields: &BTreeMap<&str, &'a str>, key: &'static str) -> Result<&'a str, AnalyzeError> {
    fields.get(key).copied().ok_or(AnalyzeError::Missing(key))
}

fn number(text: &str, key: &'static str) -> Result<f64, AnalyzeError> {
    text.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or(AnalyzeError::Invalid(key))
}

fn probability(fields: &BTreeMap<&str, &str>, key: &'static str) -> Result<f64, AnalyzeError> {
    let value = number(field(fields, key)?, key)?;
    if (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(AnalyzeError::Invalid(key))
    }
}

fn visits(fields: &BTreeMap<&str, &str>) -> Result<u64, AnalyzeError> {
    field(fields, "visits")?
        .parse()
        .map_err(|_| AnalyzeError::Invalid("visits"))
}

fn score(fields: &BTreeMap<&str, &str>) -> Result<f64, AnalyzeError> {
    number(
        fields
            .get("scoreLead")
            .or_else(|| fields.get("scoreMean"))
            .copied()
            .ok_or(AnalyzeError::Missing("scoreLead"))?,
        "scoreLead",
    )
}

fn coordinate(value: &str) -> Result<String, AnalyzeError> {
    // Board bounds are query-specific and checked by the bridge/engine.
    if value.eq_ignore_ascii_case("pass")
        || (value.len() >= 2
            && value.len() <= 3
            && value.as_bytes()[0].is_ascii_alphabetic()
            && !value.starts_with(['I', 'i'])
            && value[1..]
                .parse::<u8>()
                .is_ok_and(|n| (1..=25).contains(&n)))
    {
        Ok(value.to_owned())
    } else {
        Err(AnalyzeError::Invalid("coordinate"))
    }
}

/// Parses a complete line, including multiple `info` blocks and optional root
/// and ownership sections. Standalone root/ownership lines are also accepted.
pub fn parse(line: &str) -> Result<Report, AnalyzeError> {
    if line.len() > 1024 * 1024 {
        return Err(AnalyzeError::TooLarge);
    }
    let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
    if tokens.is_empty() {
        return Err(AnalyzeError::UnexpectedToken);
    }
    let mut report = Report::default();
    let mut index = 0;
    while index < tokens.len() {
        let section = tokens[index];
        index += 1;
        match section {
            "info" | "rootInfo" => {
                let mut fields = BTreeMap::new();
                let mut pv = None;
                while index < tokens.len() && !boundary(tokens[index]) {
                    let key = tokens[index];
                    index += 1;
                    if key == "pv" || key == "pvVisits" {
                        let start = index;
                        while index < tokens.len()
                            && !boundary(tokens[index])
                            && tokens[index] != "pvVisits"
                        {
                            index += 1;
                        }
                        if key == "pv" {
                            pv = Some(
                                tokens[start..index]
                                    .iter()
                                    .map(|v| coordinate(v))
                                    .collect::<Result<Vec<_>, _>>()?,
                            );
                        } else {
                            for value in &tokens[start..index] {
                                value
                                    .parse::<u64>()
                                    .map_err(|_| AnalyzeError::Invalid("pvVisits"))?;
                            }
                        }
                    } else {
                        let value = tokens
                            .get(index)
                            .filter(|v| !boundary(v))
                            .ok_or(AnalyzeError::UnexpectedToken)?;
                        if fields.insert(key, *value).is_some() {
                            return Err(AnalyzeError::UnexpectedToken);
                        }
                        index += 1;
                    }
                }
                if section == "rootInfo" {
                    report.root = Some(Root {
                        visits: visits(&fields)?,
                        winrate: probability(&fields, "winrate")?,
                        score_lead: score(&fields)?,
                        utility: fields
                            .get("utility")
                            .map(|v| number(v, "utility"))
                            .transpose()?
                            .unwrap_or(0.0),
                    });
                } else {
                    let pv = pv.ok_or(AnalyzeError::Missing("pv"))?;
                    if pv.is_empty() {
                        return Err(AnalyzeError::Invalid("pv"));
                    }
                    report.moves.push(Candidate {
                        coordinate: coordinate(field(&fields, "move")?)?,
                        visits: visits(&fields)?,
                        winrate: probability(&fields, "winrate")?,
                        score_lead: score(&fields)?,
                        prior: probability(&fields, "prior")?,
                        order: field(&fields, "order")?
                            .parse()
                            .map_err(|_| AnalyzeError::Invalid("order"))?,
                        pv,
                    });
                }
            }
            "ownership" | "ownershipStdev" => {
                let mut values = Vec::new();
                while index < tokens.len() && !boundary(tokens[index]) {
                    let value = number(tokens[index], "ownership")?;
                    if section == "ownership" && !(-1.0..=1.0).contains(&value) {
                        return Err(AnalyzeError::Invalid("ownership"));
                    }
                    values.push(value);
                    index += 1;
                }
                if values.is_empty() {
                    return Err(AnalyzeError::Missing("ownership"));
                }
                if section == "ownership" {
                    report.ownership = Some(values);
                }
            }
            _ => return Err(AnalyzeError::UnexpectedToken),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
