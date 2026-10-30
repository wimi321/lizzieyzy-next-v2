use document::GamePosition;
use engine_session::AnalysisRequest;
use go_core::Color;
use katago_protocol::{Player, Rules};

use crate::{AnalysisError, LiveSettings};

/// Replay only the history after the document's most recent setup/PL boundary.
/// Evaluations already use Black's perspective through the engine config;
/// neither the query nor its response needs a side-to-play conversion here.
pub(crate) fn build(
    position: &GamePosition,
    settings: &LiveSettings,
) -> Result<AnalysisRequest, AnalysisError> {
    if !position.komi.is_finite() {
        return Err(AnalysisError::InvalidPosition("komi must be finite"));
    }
    if position.analysis_moves.len() > u32::MAX as usize {
        return Err(AnalysisError::InvalidPosition("too many analysis moves"));
    }
    let size = position.board.size();
    let moves = position
        .analysis_moves
        .iter()
        .map(|mv| Ok((player(mv.color()), mv.to_gtp(size)?)))
        .collect::<Result<_, AnalysisError>>()?;
    let initial_stones = position
        .initial_stones
        .iter()
        .map(|(color, point)| Ok((player(*color), point.to_gtp(size)?)))
        .collect::<Result<_, AnalysisError>>()?;
    let report_interval = settings.report_interval.as_secs_f64();
    Ok(AnalysisRequest {
        moves,
        rules: Rules::Named(position.rules.as_katago_str().to_owned()),
        board_x_size: size.width,
        board_y_size: size.height,
        initial_stones: Some(initial_stones),
        initial_player: Some(player(position.initial_player)),
        komi: Some(position.komi),
        analyze_turns: None,
        max_visits: Some(settings.max_visits),
        include_ownership: Some(settings.include_ownership),
        include_policy: Some(settings.include_policy),
        report_during_search_every: Some(report_interval),
        first_report_during_search_after: Some(report_interval),
        priority: Some(0),
        priorities: None,
        override_settings: None,
        extensions: Default::default(),
    })
}

fn player(color: Color) -> Player {
    match color {
        Color::Black => Player::Black,
        Color::White => Player::White,
    }
}

#[cfg(test)]
mod tests;
