use super::BridgeError;
use katago_protocol::{AnalysisQuery, Player, Rules};
use std::collections::VecDeque;

pub(super) fn player(value: Player) -> &'static str {
    match value {
        Player::Black => "B",
        Player::White => "W",
    }
}
pub(super) fn to_play(query: &AnalysisQuery, turn: u32) -> Player {
    if turn > 0 {
        match query.moves[turn as usize - 1].0 {
            Player::Black => Player::White,
            Player::White => Player::Black,
        }
    } else {
        query.initial_player.unwrap_or_else(|| {
            let stones = query.initial_stones.as_deref().unwrap_or_default();
            if stones.len() > 1 && stones.iter().all(|(side, _)| *side == Player::Black) {
                Player::White
            } else {
                Player::Black
            }
        })
    }
}
pub(super) fn validate(query: &AnalysisQuery) -> Result<VecDeque<u32>, BridgeError> {
    if !(2..=25).contains(&query.board_x_size)
        || !(2..=25).contains(&query.board_y_size)
        || query.max_visits == Some(0)
        || query.komi.is_some_and(|v| !v.is_finite())
        || query
            .report_during_search_every
            .is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 86400.0)
    {
        return Err(BridgeError::InvalidQuery);
    }
    max_time(query)?;
    let mut turns = query
        .analyze_turns
        .clone()
        .unwrap_or_else(|| vec![query.moves.len() as u32]);
    if turns.is_empty()
        || turns.len() > 256
        || turns.iter().any(|v| *v as usize > query.moves.len())
    {
        return Err(BridgeError::InvalidQuery);
    }
    turns.sort_unstable();
    turns.dedup();
    Ok(turns.into())
}
pub(super) fn max_time(query: &AnalysisQuery) -> Result<Option<std::time::Duration>, BridgeError> {
    query
        .override_settings
        .as_ref()
        .and_then(|settings| settings.get("maxTime"))
        .map(|v| {
            let seconds = v
                .as_f64()
                .filter(|v| v.is_finite() && *v > 0.0 && *v <= 86400.0)
                .ok_or(BridgeError::InvalidQuery)?;
            Ok(std::time::Duration::from_secs_f64(seconds))
        })
        .transpose()
}
pub(super) fn setup(
    query: &AnalysisQuery,
    turn: u32,
    set_position: bool,
) -> Result<VecDeque<String>, BridgeError> {
    let mut commands = VecDeque::new();
    commands.push_back(if query.board_x_size == query.board_y_size {
        format!("boardsize {}", query.board_x_size)
    } else {
        format!(
            "rectangular_boardsize {} {}",
            query.board_x_size, query.board_y_size
        )
    });
    commands.push_back("clear_board".into());
    commands.push_back(format!("komi {}", query.komi.unwrap_or(7.5)));
    let rules = match &query.rules {
        Rules::Named(name) => name.clone(),
        Rules::Custom(rules) => {
            serde_json::to_string(rules).map_err(|_| BridgeError::InvalidQuery)?
        }
    };
    commands.push_back(format!("kata-set-rules {rules}"));
    if let Some(stones) = &query.initial_stones
        && !stones.is_empty()
    {
        if !set_position {
            return Err(BridgeError::SetupUnsupported);
        }
        let mut command = "set_position".to_owned();
        for (side, point) in stones {
            command.push_str(&format!(" {} {point}", player(*side)));
        }
        commands.push_back(command);
    }
    for (side, point) in query.moves.iter().take(turn as usize) {
        commands.push_back(format!("play {} {point}", player(*side)));
    }
    for command in &commands {
        gtp::Command::parse(command).map_err(|_| BridgeError::InvalidQuery)?;
    }
    Ok(commands)
}
pub(super) fn analyze(query: &AnalysisQuery, turn: u32) -> String {
    // GTP intervals are centiseconds, JSON report intervals are seconds.
    let centiseconds = (query.report_during_search_every.unwrap_or(0.1).max(0.1) * 100.0).ceil();
    format!(
        "kata-analyze {} {centiseconds} ownership true rootInfo true",
        player(to_play(query, turn))
    )
}
