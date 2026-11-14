use super::{CoachBestMove, CoachError, CoachMoveRated, CoachVerdict};
use document::GamePosition;
use go_core::{BoardSize, Color, Move};
use katago_protocol::{AnalysisResponse, MoveInfo};

fn best(response: &AnalysisResponse) -> Option<&MoveInfo> {
    response
        .move_infos
        .iter()
        .min_by_key(|candidate| candidate.order)
}

/// BLACK is forced at process startup. Both the before candidate and after root
/// already use Black's perspective; no unconditional negation is valid here.
pub(crate) fn reviewed_lead(before: &AnalysisResponse, mv: Move, size: BoardSize) -> Option<f64> {
    let best = best(before)?;
    let coordinate = mv.to_gtp(size).ok()?;
    let needed = 16.max(best.visits / 20);
    before
        .move_infos
        .iter()
        .find(|c| {
            c.coordinate.eq_ignore_ascii_case(&coordinate)
                && c.visits >= needed
                && c.score_lead.is_finite()
        })
        .map(|c| c.score_lead)
}

pub(crate) fn rank_winrate(score_lead: f64, score_stdev: Option<f64>) -> f64 {
    let spread = score_stdev
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(12.0)
        .max(1.0);
    let x = score_lead / spread;
    if !x.is_finite() {
        return f64::NAN;
    }
    // Abramowitz-Stegun 7.1.26, identical to the Java behaviour reference.
    let z = x.abs() / 2.0_f64.sqrt();
    let t = 1.0 / (1.0 + 0.3275911 * z);
    let erf = 1.0
        - ((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-z * z).exp();
    if x >= 0.0 {
        0.5 * (1.0 + erf)
    } else {
        0.5 * (1.0 - erf)
    }
}

pub(crate) fn verdict(before: f64, after: f64, lost: f64) -> CoachVerdict {
    let loss = ((before - after).max(0.0) * 100.0).round() / 100.0;
    let mut rating = if loss >= 0.20 {
        CoachVerdict::Blunder
    } else if loss >= 0.10 {
        CoachVerdict::Problem
    } else if loss >= 0.05 {
        CoachVerdict::Slow
    } else {
        CoachVerdict::Good
    };
    if !(0.05..=0.95).contains(&before) {
        rating = rating.max(if lost >= 20.0 {
            CoachVerdict::Blunder
        } else if lost >= 10.0 {
            CoachVerdict::Problem
        } else {
            CoachVerdict::Good
        });
    }
    rating
}

pub(crate) fn best_hint(
    before: &AnalysisResponse,
    position: &GamePosition,
) -> Result<CoachBestMove, CoachError> {
    let best = best(before).ok_or(CoachError::NoLegalMove)?;
    let size = position.board.size();
    let mv = Move::from_gtp(position.board.to_play(), &best.coordinate, size)
        .map_err(|_| CoachError::NoLegalMove)?;
    position
        .board
        .clone()
        .play(mv)
        .map_err(|_| CoachError::NoLegalMove)?;
    Ok(CoachBestMove {
        mv: mv.into(),
        human_share: super::selection::human_share(before, mv, size),
    })
}

pub(crate) fn rate_move(
    before: &AnalysisResponse,
    after: Option<&AnalysisResponse>,
    position: &GamePosition,
    mv: Move,
    node_id: u32,
    move_number: u32,
) -> Option<CoachMoveRated> {
    let size = position.board.size();
    let top = best(before)?;
    let lead = reviewed_lead(before, mv, size).or_else(|| after.map(|a| a.root_info.score_lead))?;
    if !top.score_lead.is_finite() || !lead.is_finite() {
        return None;
    }
    let stdev = before
        .root_info
        .human_score_stdev
        .filter(|s| s.is_finite() && *s > 0.0)
        .or_else(|| after.and_then(|r| r.root_info.human_score_stdev));
    let before_black = rank_winrate(top.score_lead, stdev);
    let after_black = rank_winrate(lead, stdev);
    let color = match mv {
        Move::Play(c, _) | Move::Pass(c) => c,
    };
    let own = |black| {
        if color == Color::Black {
            black
        } else {
            1.0 - black
        }
    };
    let points_lost = (if color == Color::Black {
        top.score_lead - lead
    } else {
        lead - top.score_lead
    })
    .max(0.0);
    let mut rating = verdict(own(before_black), own(after_black), points_lost);
    let hint = best_hint(before, position).ok()?;
    let share = super::selection::human_share(before, mv, size);
    if rating == CoachVerdict::Good
        && hint.mv == mv.into()
        && share.is_some_and(|p| p < 0.1)
        && let Some(policy) = before.human_policy.as_ref()
        && let Some((index, _)) = policy
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_finite() && **p >= 0.0)
            .max_by(|(ai, a), (bi, b)| a.total_cmp(b).then_with(|| bi.cmp(ai)))
    {
        let area = usize::from(size.width) * usize::from(size.height);
        let common = if index == area {
            Move::Pass(color)
        } else {
            Move::Play(
                color,
                go_core::Point {
                    x: (index % usize::from(size.width)) as u8,
                    y: (index / usize::from(size.width)) as u8,
                },
            )
        };
        if common != mv
            && let Some(common_lead) = reviewed_lead(before, common, size)
            && own(before_black) - own(rank_winrate(common_lead, stdev)) >= 0.05
        {
            rating = CoachVerdict::Brilliant;
        }
    }
    Some(CoachMoveRated {
        node_id,
        move_number,
        color: color.into(),
        verdict: rating,
        winrate_before: before_black,
        winrate_after: after_black,
        winrate_change: after_black - before_black,
        points_lost,
        human_share: share,
        best: hint,
    })
}

#[cfg(test)]
mod tests;
