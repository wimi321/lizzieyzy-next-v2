use super::{CoachError, CoachRank};
use document::GamePosition;
use go_core::{Color, Move, Point};
use katago_protocol::{AnalysisResponse, MoveInfo};

#[derive(Clone, Debug)]
pub(crate) struct PolicyCandidate {
    pub mv: Move,
    pub coordinate: String,
    pub probability: f64,
}

fn policy_candidates(
    response: &AnalysisResponse,
    position: &GamePosition,
    allow_pass: bool,
) -> Result<Vec<PolicyCandidate>, CoachError> {
    let size = position.board.size();
    let area = usize::from(size.width) * usize::from(size.height);
    let policy = response
        .human_policy
        .as_ref()
        .filter(|p| p.len() == area + 1)
        .ok_or(CoachError::MissingHumanPolicy)?;
    let color = position.board.to_play();
    let mut candidates = Vec::new();
    for (index, probability) in policy.iter().copied().enumerate() {
        if !probability.is_finite() || probability <= 0.0 || (index == area && !allow_pass) {
            continue;
        }
        let mv = if index == area {
            Move::Pass(color)
        } else {
            Move::Play(
                color,
                Point {
                    x: (index % usize::from(size.width)) as u8,
                    y: (index / usize::from(size.width)) as u8,
                },
            )
        };
        let coordinate = mv.to_gtp(size).map_err(|_| CoachError::NoLegalMove)?;
        candidates.push(PolicyCandidate {
            mv,
            coordinate,
            probability,
        });
    }
    Ok(candidates)
}

fn legal(position: &GamePosition, mv: Move) -> bool {
    position.board.clone().play(mv).is_ok()
}

pub(crate) fn raw_temperature(move_number: usize, area: usize) -> f64 {
    0.70 + 0.15 * 0.5_f64.powf(move_number as f64 / 80.0 * 19.0 / (area.max(1) as f64).sqrt())
}

fn raw_weights(candidates: &[PolicyCandidate], temperature: f64) -> Vec<f64> {
    let max = candidates.iter().map(|c| c.probability).fold(0.0, f64::max);
    let total: f64 = candidates.iter().map(|c| c.probability).sum();
    if max <= 0.0 || !total.is_finite() {
        return vec![0.0; candidates.len()];
    }
    let threshold = (0.01_f64.ln() + (total / max).ln()).min(0.0);
    candidates
        .iter()
        .map(|c| {
            let relative = (c.probability / max).ln();
            (if relative > threshold {
                relative
            } else {
                (relative - threshold) / temperature.max(0.0001) + threshold
            })
            .exp()
        })
        .collect()
}

fn sample_index(weights: &[f64], random: &mut u64) -> Option<usize> {
    let total: f64 = weights.iter().sum();
    if total <= 0.0 || !total.is_finite() {
        return None;
    }
    let target = crate::game::random_unit(random) * total;
    let mut sum = 0.0;
    for (index, weight) in weights.iter().enumerate() {
        sum += weight;
        if target < sum {
            return Some(index);
        }
    }
    weights.iter().rposition(|weight| *weight > 0.0)
}

/// Passes are chosen by the ordinary KataGo pass check, never by policy noise.
/// Sampling removes a rejected point and draws again against the remaining mass.
pub(crate) fn sample_raw_policy(
    response: &AnalysisResponse,
    position: &GamePosition,
    random: &mut u64,
) -> Result<Move, CoachError> {
    let mut candidates = policy_candidates(response, position, false)?;
    let size = position.board.size();
    let temperature = raw_temperature(
        position.moves.len(),
        usize::from(size.width) * usize::from(size.height),
    );
    while !candidates.is_empty() {
        let weights = raw_weights(&candidates, temperature);
        let index = sample_index(&weights, random).ok_or(CoachError::NoLegalMove)?;
        let chosen = candidates.remove(index);
        if legal(position, chosen.mv) {
            return Ok(chosen.mv);
        }
    }
    Err(CoachError::NoLegalMove)
}

pub(crate) fn candidate_pool(
    response: &AnalysisResponse,
    position: &GamePosition,
) -> Result<Vec<PolicyCandidate>, CoachError> {
    let allow_pass = pass_check_needed(position);
    let mut candidates = policy_candidates(response, position, allow_pass)?;
    candidates.retain(|c| legal(position, c.mv));
    candidates.sort_by(|a, b| {
        b.probability
            .total_cmp(&a.probability)
            .then_with(|| a.coordinate.cmp(&b.coordinate))
    });
    let total: f64 = candidates.iter().map(|c| c.probability).sum();
    let top = candidates.first().map_or(0.0, |c| c.probability);
    let mut mass = 0.0;
    let mut pool = Vec::new();
    for candidate in candidates {
        if pool.len() >= 12 || candidate.probability < top * 0.01 {
            break;
        }
        mass += candidate.probability;
        pool.push(candidate);
        if mass / total >= 0.95 {
            break;
        }
    }
    let size = position.board.size();
    let color = position.board.to_play();
    let mut extras = Vec::new();
    if let Some(best) = response.move_infos.iter().min_by_key(|c| c.order)
        && let Ok(mv) = Move::from_gtp(color, &best.coordinate, size)
    {
        extras.push(mv);
    }
    let area = usize::from(size.width) * usize::from(size.height);
    if let Some(policy) = response.policy.as_ref().filter(|p| p.len() == area + 1)
        && let Some((index, _)) = policy
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_finite() && **p >= 0.0)
            .max_by(|(ai, a), (bi, b)| a.total_cmp(b).then_with(|| bi.cmp(ai)))
    {
        extras.push(if index == area {
            Move::Pass(color)
        } else {
            Move::Play(
                color,
                Point {
                    x: (index % usize::from(size.width)) as u8,
                    y: (index / usize::from(size.width)) as u8,
                },
            )
        });
    }
    for mv in extras {
        if (matches!(mv, Move::Pass(_)) && !allow_pass)
            || !legal(position, mv)
            || pool.iter().any(|c| c.mv == mv)
        {
            continue;
        }
        let probability = human_share(response, mv, size).unwrap_or(0.0);
        pool.push(PolicyCandidate {
            mv,
            probability,
            coordinate: mv.to_gtp(size).map_err(|_| CoachError::NoLegalMove)?,
        });
    }
    if pool.is_empty() {
        Err(CoachError::NoLegalMove)
    } else {
        Ok(pool)
    }
}

pub(crate) fn human_share(
    response: &AnalysisResponse,
    mv: Move,
    size: go_core::BoardSize,
) -> Option<f64> {
    let area = usize::from(size.width) * usize::from(size.height);
    let index = match mv {
        Move::Pass(_) => area,
        Move::Play(_, p) => usize::from(p.y) * usize::from(size.width) + usize::from(p.x),
    };
    response
        .human_policy
        .as_ref()
        .filter(|p| p.len() == area + 1)
        .and_then(|p| p.get(index).copied())
        .filter(|p| p.is_finite() && *p >= 0.0)
        .map(|p| p.min(1.0))
}

pub(crate) fn human_top_share(response: &AnalysisResponse) -> f64 {
    let Some(policy) = &response.human_policy else {
        return 0.45;
    };
    let weights = policy
        .iter()
        .take(policy.len().saturating_sub(1))
        .copied()
        .filter(|p| p.is_finite() && *p > 0.0);
    let (max, sum) = weights.fold((0.0_f64, 0.0), |(max, sum), p| (max.max(p), sum + p));
    if sum > 0.0 && sum.is_finite() {
        max / sum
    } else {
        0.45
    }
}

pub(crate) fn human_pass_share(response: &AnalysisResponse) -> f64 {
    let Some(policy) = &response.human_policy else {
        return 0.0;
    };
    let sum: f64 = policy
        .iter()
        .copied()
        .filter(|p| p.is_finite() && *p > 0.0)
        .sum();
    let pass = policy
        .last()
        .copied()
        .filter(|p| p.is_finite() && *p > 0.0)
        .unwrap_or(0.0);
    if sum > 0.0 && sum.is_finite() {
        pass / sum
    } else {
        0.0
    }
}

#[derive(Clone, Copy)]
enum QualityKind {
    Utility,
    Score,
    Winrate,
}

fn quality_kind(response: &AnalysisResponse) -> QualityKind {
    if response
        .move_infos
        .iter()
        .any(|c| c.utility.is_some_and(f64::is_finite))
    {
        QualityKind::Utility
    } else if response.move_infos.iter().any(|c| c.score_lead.is_finite()) {
        QualityKind::Score
    } else {
        QualityKind::Winrate
    }
}

fn quality(candidate: &MoveInfo, kind: QualityKind, color: Color) -> Option<f64> {
    let value = match kind {
        QualityKind::Utility => candidate.utility?,
        QualityKind::Score => candidate.score_lead,
        QualityKind::Winrate => candidate.winrate,
    };
    value
        .is_finite()
        .then_some(if color == Color::Black { value } else { -value })
}

fn max_loss(rank: CoachRank, kind: QualityKind) -> f64 {
    let band = match rank {
        CoachRank::Dan(7..) => 0,
        CoachRank::Dan(4..) => 1,
        CoachRank::Dan(_) => 2,
        CoachRank::Kyu(1..=5) => 3,
        CoachRank::Kyu(6..=10) => 4,
        CoachRank::Kyu(_) => 5,
    };
    match kind {
        QualityKind::Utility => [0.32, 0.45, 0.65, 0.85, 1.15, 1.80][band],
        QualityKind::Score => [3.0, 4.0, 5.5, 7.0, 10.0, 15.0][band],
        QualityKind::Winrate => [0.10, 0.14, 0.20, 0.25, 0.34, 0.48][band],
    }
}

fn candidate_losses(
    pool: &[PolicyCandidate],
    response: &AnalysisResponse,
    color: Color,
) -> (QualityKind, Vec<Option<f64>>) {
    let kind = quality_kind(response);
    let best = response
        .move_infos
        .iter()
        .filter_map(|c| quality(c, kind, color))
        .fold(f64::NEG_INFINITY, f64::max);
    let losses = pool
        .iter()
        .map(|candidate| {
            response
                .move_infos
                .iter()
                .find(|c| c.coordinate.eq_ignore_ascii_case(&candidate.coordinate))
                .and_then(|c| quality(c, kind, color))
                .map(|q| (best - q).max(0.0))
        })
        .collect();
    (kind, losses)
}

pub(crate) fn needs_deeper(
    pool: &[PolicyCandidate],
    response: &AnalysisResponse,
    color: Color,
    rank: CoachRank,
) -> bool {
    if rank.elite() {
        return true;
    }
    let (kind, losses) = candidate_losses(pool, response, color);
    losses
        .iter()
        .any(|loss| loss.is_none_or(|loss| loss > max_loss(rank, kind) * 0.55))
}

pub(crate) fn select_searched_move(
    pool: &[PolicyCandidate],
    response: &AnalysisResponse,
    position: &GamePosition,
    rank: CoachRank,
    random: &mut u64,
) -> Result<Move, CoachError> {
    let (kind, losses) = candidate_losses(pool, response, position.board.to_play());
    let maximum = max_loss(rank, kind);
    let temperature = if position.moves.len() <= 60 {
        1.25
    } else if position.moves.len() <= 160 {
        1.05
    } else {
        0.90
    };
    let weights: Vec<_> = pool
        .iter()
        .zip(losses)
        .map(|(c, loss)| {
            // Verification may examine pass, but only the ordinary pass check
            // is allowed to choose it. HumanSL policy noise cannot end a game.
            if matches!(c.mv, Move::Pass(_)) {
                return 0.0;
            }
            loss.filter(|loss| *loss <= maximum).map_or(0.0, |loss| {
                c.probability.powf(1.0 / temperature) * (-loss / maximum).exp()
            })
        })
        .collect();
    if let Some(index) = sample_index(&weights, random)
        && let Some(candidate) = pool.get(index)
        && legal(position, candidate.mv)
    {
        return Ok(candidate.mv);
    }
    // Verified search may prefer an added point with zero human mass. Only use
    // that legal, permitted candidate; never invent pass for invalid output.
    response
        .move_infos
        .iter()
        .filter_map(|info| {
            pool.iter()
                .find(|c| c.coordinate.eq_ignore_ascii_case(&info.coordinate))
                .filter(|c| !matches!(c.mv, Move::Pass(_)) && legal(position, c.mv))
                .map(|c| (info.order, c.mv))
        })
        .min_by_key(|(order, _)| *order)
        .map(|(_, mv)| mv)
        .ok_or(CoachError::NoLegalMove)
}

pub(crate) fn pass_check_needed(position: &GamePosition) -> bool {
    let size = position.board.size();
    position.moves.len() as f64 >= 0.4 * f64::from(size.width) * f64::from(size.height)
        || matches!(position.moves.last(), Some(Move::Pass(_)))
}

pub(crate) fn pass_override(
    response: &AnalysisResponse,
    position: &GamePosition,
    pass_share: f64,
) -> Option<Move> {
    if !pass_check_needed(position) {
        return None;
    }
    let best = response.move_infos.iter().min_by_key(|c| c.order)?;
    let color = position.board.to_play();
    let best_move = Move::from_gtp(color, &best.coordinate, position.board.size()).ok()?;
    if matches!(best_move, Move::Pass(_)) {
        return Some(best_move);
    }
    if matches!(position.moves.last(), Some(Move::Pass(_)))
        && let Some(pass) = response
            .move_infos
            .iter()
            .find(|c| c.coordinate.eq_ignore_ascii_case("pass"))
    {
        let loss = if color == Color::Black {
            best.score_lead - pass.score_lead
        } else {
            pass.score_lead - best.score_lead
        };
        if loss.is_finite() && loss <= 1.0 {
            return Some(Move::Pass(color));
        }
    }
    (pass_share >= 0.5 && legal(position, best_move)).then_some(best_move)
}

#[cfg(test)]
mod tests;
