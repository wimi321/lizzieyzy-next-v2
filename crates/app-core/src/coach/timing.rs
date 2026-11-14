use super::{CoachPace, CoachRank};
use document::GamePosition;
use go_core::Color;
use katago_protocol::AnalysisResponse;
use std::time::Duration;

pub(crate) fn should_resign(
    rank: CoachRank,
    response: &AnalysisResponse,
    position: &GamePosition,
    hopeless_turns: &mut u32,
) -> bool {
    let strength = match rank {
        CoachRank::Kyu(_) => 0.0,
        CoachRank::Dan(dan) => (f64::from(dan) - 1.0) / 8.0,
    }
    .clamp(0.0, 1.0);
    let maximum_winrate = 0.005 + 0.005 * strength;
    let minimum_behind = 40.0 - 20.0 * strength;
    let required = (20.0 - 10.0 * strength).round() as u32;
    let (winrate, lead) = if position.board.to_play() == Color::Black {
        (response.root_info.winrate, response.root_info.score_lead)
    } else {
        (
            1.0 - response.root_info.winrate,
            -response.root_info.score_lead,
        )
    };
    if !winrate.is_finite() || !lead.is_finite() {
        *hopeless_turns = 0;
        return false;
    }
    let size = position.board.size();
    if position.moves.len() as f64 >= 0.4 * f64::from(size.width) * f64::from(size.height)
        && winrate <= maximum_winrate
        && lead <= -minimum_behind
    {
        *hopeless_turns = hopeless_turns.saturating_add(1);
    } else {
        *hopeless_turns = 0;
    }
    *hopeless_turns >= required
}

pub(crate) fn think_delay(
    pace: CoachPace,
    top_share: f64,
    move_number: usize,
    area: usize,
    random: &mut u64,
) -> Duration {
    let top = if top_share.is_finite() {
        top_share.clamp(0.0, 1.0)
    } else {
        0.45
    };
    let base = if (move_number as f64) < area as f64 * 0.1 {
        1.0
    } else {
        1.6
    };
    let difficulty = 0.6 + 2.2 * (1.0 - top).powf(1.5);
    let jitter = 0.75 + 0.6 * crate::game::random_unit(random);
    let natural = (base * difficulty * jitter).clamp(0.4, 8.0);
    let (factor, min, max) = match pace {
        CoachPace::Natural => (1.0, 0.4, 8.0),
        CoachPace::Fast => (0.4, 0.25, 1.8),
        CoachPace::Slow => (2.0, 0.8, 14.0),
    };
    Duration::from_secs_f64((natural * factor).clamp(min, max))
}

/// Bound the next verification by observed throughput and the shared move
/// deadline. Caller additionally caps the number of deepening rounds at four.
pub(crate) fn adaptive_visits(previous: u64, elapsed: Duration, remaining: Duration) -> u64 {
    let usable = remaining.saturating_sub(Duration::from_millis(450));
    if elapsed.is_zero() || usable < Duration::from_millis(750) || previous >= 4096 {
        return previous;
    }
    let minimum = previous.saturating_add(16.max(previous / 4)).min(4096);
    let estimated = previous as f64 * usable.as_secs_f64() / elapsed.as_secs_f64() * 0.85;
    if estimated < minimum as f64 {
        previous
    } else {
        (estimated.floor() as u64).min(4096)
    }
}

#[cfg(test)]
mod tests;
