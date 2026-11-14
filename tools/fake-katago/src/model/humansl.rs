//! Deterministic HumanSL data and root candidate restrictions for tests.
use super::*;

#[derive(Clone)]
pub(super) struct Allowance {
    player: u8,
    moves: Vec<String>,
    depth: u64,
}

pub(super) fn profile(value: &Value) -> Result<Option<String>, QueryError> {
    value["overrideSettings"]
        .get("humanSLProfile")
        .map(|value| {
            let text = value.as_str().ok_or_else(|| invalid("humanSLProfile"))?;
            let valid = text
                .strip_prefix("rank_")
                .and_then(|rank| {
                    let number = rank.get(..rank.len().checked_sub(1)?)?.parse::<u8>().ok()?;
                    match rank.as_bytes().last() {
                        Some(b'k') => Some((1..=20).contains(&number)),
                        Some(b'd') => Some((1..=9).contains(&number)),
                        _ => None,
                    }
                })
                .unwrap_or(false);
            if valid {
                Ok(text.to_owned())
            } else {
                Err(invalid("humanSLProfile"))
            }
        })
        .transpose()
}

pub(super) fn allowances(
    value: &Value,
    width: usize,
    height: usize,
) -> Result<Vec<Allowance>, QueryError> {
    let Some(values) = value.get("allowMoves") else {
        return Ok(Vec::new());
    };
    let board = Board {
        cells: Vec::new(),
        width,
        height,
    };
    values
        .as_array()
        .ok_or_else(|| invalid("allowMoves"))?
        .iter()
        .map(|value| {
            let player = color(&value["player"]).ok_or_else(|| invalid("allowMoves"))?;
            let depth = value["untilDepth"]
                .as_u64()
                .ok_or_else(|| invalid("allowMoves"))?;
            let moves = value["moves"]
                .as_array()
                .ok_or_else(|| invalid("allowMoves"))?
                .iter()
                .map(|value| {
                    let text = value.as_str().ok_or_else(|| invalid("allowMoves"))?;
                    board.index(text).ok_or_else(|| invalid("allowMoves"))?;
                    Ok(text.to_owned())
                })
                .collect::<Result<Vec<_>, QueryError>>()?;
            Ok(Allowance {
                player,
                moves,
                depth,
            })
        })
        .collect()
}

impl Query {
    pub(super) fn apply_human_policy(
        &self,
        response: &mut Value,
        board: &Board,
        legal: &[bool],
        hash: u64,
        turn: usize,
    ) {
        let Some(profile) = self.human_profile.as_deref().filter(|_| self.human_model) else {
            return;
        };
        let salt = profile
            .bytes()
            .fold(hash, |hash, byte| mix(hash ^ u64::from(byte)));
        let mut policy: Vec<f64> = legal
            .iter()
            .enumerate()
            .map(|(i, allowed)| {
                if *allowed {
                    1.0 + (mix(salt.wrapping_add(i as u64)) % 1000) as f64
                } else {
                    -1.0
                }
            })
            .collect();
        policy.push(1.0);
        let total: f64 = policy.iter().filter(|v| **v > 0.0).sum();
        for probability in &mut policy {
            if *probability > 0.0 {
                *probability /= total;
            }
        }
        let step = self.script.turn(turn);
        if let Some(scripted) = step.and_then(|step| step.human_policy.as_ref()) {
            policy.clone_from(scripted);
        }
        if self.policy {
            response["humanPolicy"] = json!(policy);
        }
        response["rootInfo"]["humanWinrate"] = response["rootInfo"]["winrate"].clone();
        response["rootInfo"]["humanScoreMean"] = response["rootInfo"]["scoreLead"].clone();
        response["rootInfo"]["humanScoreStdev"] = json!(8.0 + (salt % 9) as f64);
        if let Some(step) = step {
            step.apply_human_root(response);
        }
        if let Some(infos) = response["moveInfos"].as_array_mut() {
            for info in infos {
                if let Some(coordinate) = info["move"].as_str() {
                    let prior = step
                        .and_then(|step| step.human_prior(coordinate))
                        .or_else(|| {
                            let index = board.index(coordinate)?.unwrap_or(legal.len());
                            policy.get(index).copied()
                        });
                    if let Some(prior) = prior {
                        info["humanPrior"] = json!(prior);
                    }
                }
            }
        }
    }

    pub(super) fn restrict_moves(
        &self,
        response: &mut Value,
        board: &Board,
        legal: &[bool],
        player: u8,
        visits: u64,
    ) {
        let allowances: Vec<_> = self
            .allow_moves
            .iter()
            .filter(|a| a.player == player && a.depth > 0)
            .collect();
        if allowances.is_empty() {
            return;
        }
        let mut allowed = Vec::new();
        for coordinate in allowances.iter().flat_map(|a| &a.moves) {
            if board
                .index(coordinate)
                .is_some_and(|index| index.is_none_or(|index| legal[index]))
                && !allowed
                    .iter()
                    .any(|p: &&String| p.eq_ignore_ascii_case(coordinate))
            {
                allowed.push(coordinate);
            }
        }
        let old = response["moveInfos"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        allowed.sort_by_key(|coordinate| {
            old.iter()
                .position(|info| {
                    info["move"]
                        .as_str()
                        .is_some_and(|mv| mv.eq_ignore_ascii_case(coordinate))
                })
                .unwrap_or(usize::MAX)
        });
        let root = &response["rootInfo"];
        let infos: Vec<_> = allowed
            .iter()
            .enumerate()
            .map(|(order, coordinate)| {
                let mut info = old
                    .iter()
                    .find(|info| {
                        info["move"]
                            .as_str()
                            .is_some_and(|mv| mv.eq_ignore_ascii_case(coordinate))
                    })
                    .cloned()
                    .unwrap_or_else(|| {
                        json!({
                            "move": coordinate, "visits": visits / allowed.len() as u64,
                            "winrate": root["winrate"], "scoreLead": root["scoreLead"],
                            "prior": 1.0 / allowed.len() as f64, "pv": [coordinate],
                        })
                    });
                info["order"] = json!(order);
                if info.get("humanPrior").is_none()
                    && self.human_model
                    && self.human_profile.is_some()
                {
                    let index = board.index(coordinate).flatten().unwrap_or(legal.len());
                    if let Some(prior) = response["humanPolicy"].get(index) {
                        info["humanPrior"] = prior.clone();
                    }
                }
                info
            })
            .collect();
        response["moveInfos"] = json!(infos);
    }
}

#[cfg(test)]
mod tests;
