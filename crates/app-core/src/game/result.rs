/// Scores lie on the integer-board-score minus komi lattice. This is an engine
/// estimate; no manual territory adjudication is performed here.
pub(crate) fn score_result(score: f64, komi: f64) -> Option<String> {
    if !score.is_finite() || !komi.is_finite() {
        return None;
    }
    let aligned = (score + komi).round() - komi;
    if !aligned.is_finite() {
        return None;
    }
    if aligned == 0.0 {
        return Some("0".into());
    }
    let winner = if aligned > 0.0 { "B" } else { "W" };
    // Preserve the supplied komi precision, including small fractional values,
    // while avoiding binary subtraction tails in ordinary decimal scores.
    let komi_text = komi.to_string();
    let precision = komi_text
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let amount = format!("{:.*}", precision, aligned.abs());
    let amount = if precision == 0 {
        amount.as_str()
    } else {
        amount.trim_end_matches('0').trim_end_matches('.')
    };
    Some(format!("{winner}+{amount}"))
}

pub(crate) fn game_file_name(timestamp: &str, black: &str, white: &str) -> Option<String> {
    if timestamp.len() != 14 || !timestamp.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(format!(
        "{}-{}-{}-{}_{}_vs_{}.sgf",
        &timestamp[..4],
        &timestamp[4..6],
        &timestamp[6..8],
        &timestamp[8..],
        safe_name(black),
        safe_name(white)
    ))
}

fn safe_name(name: &str) -> String {
    let mut clean = String::new();
    for c in name.chars() {
        let c = if c.is_control() || "/\\<>:\"|?*".contains(c) {
            '_'
        } else {
            c
        };
        if clean.len() + c.len_utf8() > 64 {
            break;
        }
        clean.push(c);
    }
    let clean = clean.trim().trim_end_matches('.').trim();
    if clean.is_empty() {
        "Player".into()
    } else {
        clean.into()
    }
}
