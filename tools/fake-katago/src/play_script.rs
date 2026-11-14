//! Test-only per-position output overrides. Values are supplied in Black's
//! perspective, before the normal simulated engine perspective conversion.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};
use thiserror::Error;

#[derive(Clone, Default)]
pub struct PlayScript {
    default: Option<Turn>,
    turns: BTreeMap<usize, Turn>,
    profiles: BTreeMap<String, PlayScript>,
}

#[derive(Clone, Default)]
pub struct Turn {
    pub error: Option<String>,
    pub hang: bool,
    gate_file: Option<PathBuf>,
    winrate: Option<f64>,
    score_lead: Option<f64>,
    candidates: Option<Vec<Candidate>>,
    policy: Option<Vec<f64>>,
    pub human_policy: Option<Vec<f64>>,
    human_winrate: Option<f64>,
    human_score_mean: Option<f64>,
    human_score_stdev: Option<f64>,
}

#[derive(Clone)]
struct Candidate {
    coordinate: String,
    winrate: Option<f64>,
    score_lead: Option<f64>,
    visit_share: f64,
    human_prior: Option<f64>,
    pv: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("cannot read play script: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid play script JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid play script field: {0}")]
    Invalid(&'static str),
}

impl PlayScript {
    pub fn load(path: Option<&Path>) -> Result<Self, ScriptError> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        Self::parse(&serde_json::from_slice(&std::fs::read(path)?)?)
    }

    fn parse(value: &Value) -> Result<Self, ScriptError> {
        if !value.is_object() {
            return Err(ScriptError::Invalid("root"));
        }
        let default = value.get("default").map(Turn::parse).transpose()?;
        let mut turns = BTreeMap::new();
        if let Some(entries) = value.get("turns") {
            for (key, entry) in entries.as_object().ok_or(ScriptError::Invalid("turns"))? {
                let turn = key.parse().map_err(|_| ScriptError::Invalid("turn"))?;
                turns.insert(turn, Turn::parse(entry)?);
            }
        }
        let mut profiles = BTreeMap::new();
        if let Some(entries) = value.get("profiles") {
            for (profile, entry) in entries
                .as_object()
                .ok_or(ScriptError::Invalid("profiles"))?
            {
                profiles.insert(profile.clone(), Self::parse(entry)?);
            }
        }
        Ok(Self {
            default,
            turns,
            profiles,
        })
    }

    pub fn for_profile(&self, profile: Option<&str>) -> &Self {
        profile
            .and_then(|name| self.profiles.get(name))
            .unwrap_or(self)
    }

    pub fn turn(&self, turn: usize) -> Option<&Turn> {
        self.turns.get(&turn).or(self.default.as_ref())
    }

    pub fn has_gate(&self) -> bool {
        self.default
            .iter()
            .chain(self.turns.values())
            .any(|turn| turn.gate_file.is_some())
            || self.profiles.values().any(Self::has_gate)
    }
}

impl Turn {
    fn parse(value: &Value) -> Result<Self, ScriptError> {
        if !value.is_object() {
            return Err(ScriptError::Invalid("turn"));
        }
        let error = value
            .get("error")
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or(ScriptError::Invalid("error"))
            })
            .transpose()?;
        let hang = value
            .get("hang")
            .map(|v| v.as_bool().ok_or(ScriptError::Invalid("hang")))
            .transpose()?
            .unwrap_or(false);
        let gate_file = value
            .get("gateFile")
            .map(|v| {
                v.as_str()
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
                    .ok_or(ScriptError::Invalid("gateFile"))
            })
            .transpose()?;
        let candidates = value
            .get("candidates")
            .map(|v| {
                v.as_array()
                    .ok_or(ScriptError::Invalid("candidates"))?
                    .iter()
                    .map(Candidate::parse)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        Ok(Self {
            error,
            hang,
            gate_file,
            winrate: probability(value, "winrate")?,
            score_lead: number(value, "scoreLead")?,
            candidates,
            policy: policy(value, "policy")?,
            human_policy: policy(value, "humanPolicy")?,
            human_winrate: probability(value, "humanWinrate")?,
            human_score_mean: number(value, "humanScoreMean")?,
            human_score_stdev: number(value, "humanScoreStdev")?,
        })
    }

    pub fn blocked(&self) -> bool {
        self.hang || self.gate_file.as_ref().is_some_and(|path| !path.is_file())
    }

    pub fn apply(&self, response: &mut Value, visits: u64) {
        if let Some(policy) = &self.policy {
            response["policy"] = json!(policy);
        }
        let root = &mut response["rootInfo"];
        override_info(root, self.winrate, self.score_lead);
        if let Some(candidates) = &self.candidates {
            let winrate = root["winrate"].as_f64().unwrap_or(0.5);
            let score = root["scoreLead"].as_f64().unwrap_or(0.0);
            response["moveInfos"] = Value::Array(
                candidates
                    .iter()
                    .enumerate()
                    .map(|(order, candidate)| {
                        let count = (visits as f64 * candidate.visit_share) as u64;
                        let winrate = candidate.winrate.unwrap_or(winrate);
                        let score = candidate.score_lead.unwrap_or(score);
                        json!({
                            "move": candidate.coordinate, "visits": count,
                            "edgeVisits": count, "winrate": winrate,
                            "scoreMean": score, "scoreLead": score,
                            "scoreSelfplay": score, "scoreStdev": 12.0,
                            "utility": 2.0 * winrate - 1.0,
                            "prior": candidate.visit_share, "order": order,
                            "pv": candidate.pv,
                        })
                    })
                    .collect(),
            );
        } else if let Some(infos) = response["moveInfos"].as_array_mut() {
            for info in infos {
                override_info(info, self.winrate, self.score_lead);
            }
        }
    }

    pub fn apply_human_root(&self, response: &mut Value) {
        for (name, value) in [
            ("humanWinrate", self.human_winrate),
            ("humanScoreMean", self.human_score_mean),
            ("humanScoreStdev", self.human_score_stdev),
        ] {
            if let Some(value) = value {
                response["rootInfo"][name] = json!(value);
            }
        }
    }

    pub fn human_prior(&self, coordinate: &str) -> Option<f64> {
        self.candidates
            .as_ref()?
            .iter()
            .find(|candidate| candidate.coordinate.eq_ignore_ascii_case(coordinate))?
            .human_prior
    }
}

impl Candidate {
    fn parse(value: &Value) -> Result<Self, ScriptError> {
        let coordinate = value["move"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or(ScriptError::Invalid("candidate.move"))?
            .to_owned();
        let pv = match value.get("pv") {
            None => vec![coordinate.clone()],
            Some(value) => value
                .as_array()
                .ok_or(ScriptError::Invalid("pv"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or(ScriptError::Invalid("pv coordinate"))
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        Ok(Self {
            coordinate,
            winrate: probability(value, "winrate")?,
            score_lead: number(value, "scoreLead")?,
            visit_share: probability(value, "visitShare")?.unwrap_or(1.0),
            human_prior: probability(value, "humanPrior")?,
            pv,
        })
    }
}

fn policy(value: &Value, field: &'static str) -> Result<Option<Vec<f64>>, ScriptError> {
    value
        .get(field)
        .map(|v| {
            v.as_array()
                .ok_or(ScriptError::Invalid(field))?
                .iter()
                .map(|v| {
                    v.as_f64()
                        .filter(|v| *v == -1.0 || (0.0..=1.0).contains(v))
                        .ok_or(ScriptError::Invalid(field))
                })
                .collect()
        })
        .transpose()
}

fn number(value: &Value, field: &'static str) -> Result<Option<f64>, ScriptError> {
    value
        .get(field)
        .map(|v| {
            v.as_f64()
                .filter(|v| v.is_finite())
                .ok_or(ScriptError::Invalid(field))
        })
        .transpose()
}

fn probability(value: &Value, field: &'static str) -> Result<Option<f64>, ScriptError> {
    let result = number(value, field)?;
    if result.is_some_and(|v| !(0.0..=1.0).contains(&v)) {
        return Err(ScriptError::Invalid(field));
    }
    Ok(result)
}

fn override_info(info: &mut Value, winrate: Option<f64>, score: Option<f64>) {
    if let Some(winrate) = winrate {
        info["winrate"] = json!(winrate);
        info["rawWinrate"] = json!(winrate);
        info["utility"] = json!(2.0 * winrate - 1.0);
    }
    if let Some(score) = score {
        for key in ["scoreLead", "scoreMean", "scoreSelfplay", "rawLead"] {
            info[key] = json!(score);
        }
    }
}

#[cfg(test)]
mod tests;
