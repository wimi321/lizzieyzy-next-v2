use std::collections::{BTreeSet, HashSet};

use go_core::{Color, Move, Position};

use crate::{Error, Language, MAX_PROMPT_CHARACTERS, board};

/// Analysis values are supplied in Black's perspective, exactly as retained by app-core.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub mv: Move,
    pub order: u32,
    pub visits: u64,
    pub winrate: f64,
    pub score_lead: Option<f64>,
    pub pv: Vec<Move>,
}

#[derive(Clone, Debug)]
pub struct PositionInput {
    pub node_id: u32,
    /// Number of already played moves; the actual next move is move_number + 1.
    pub move_number: u32,
    /// Full go-core state, including rule and repetition history, not a reconstructed diagram.
    pub board: Position,
    pub root_visits: u64,
    pub actual_move: Option<Move>,
    /// Starts with the actual next move, when one exists.
    pub continuation: Vec<Move>,
    pub candidates: Vec<Candidate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedPosition {
    pub node_id: u32,
    pub move_number: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    pub text: String,
    pub positions: Vec<SelectedPosition>,
    pub analyzed: usize,
    pub selected: usize,
    pub omitted: usize,
    pub allowed_coordinates: BTreeSet<String>,
}

fn winrate(candidate: &Candidate, color: Color) -> f64 {
    (match color {
        Color::Black => candidate.winrate,
        Color::White => 1.0 - candidate.winrate,
    }) * 100.0
}

fn score(candidate: &Candidate, color: Color) -> Option<f64> {
    candidate
        .score_lead
        .map(|score| if color == Color::Black { score } else { -score })
}

fn sorted_candidates(position: &PositionInput) -> Vec<&Candidate> {
    let mut candidates: Vec<_> = position.candidates.iter().collect();
    candidates.sort_by_key(|candidate| candidate.order);
    candidates
}

fn losses(position: &PositionInput) -> (Option<f64>, Option<f64>) {
    let candidates = sorted_candidates(position);
    let Some(best) = candidates.first() else {
        return (None, None);
    };
    let Some(actual) = position
        .actual_move
        .and_then(|mv| candidates.iter().find(|candidate| candidate.mv == mv))
    else {
        return (None, None);
    };
    let color = position.board.to_play();
    if actual.mv.color() != color {
        return (None, None);
    }
    let score_loss = score(best, color)
        .zip(score(actual, color))
        .map(|(best, actual)| (best - actual).max(0.0));
    let winrate_loss = (winrate(best, color) - winrate(actual, color)).max(0.0);
    (score_loss, Some(winrate_loss))
}

/// Java's importance fallback: score loss, otherwise winrate points / 10,
/// otherwise best-candidate visits / 1_000_000. Unknown loss is never fabricated.
fn importance(position: &PositionInput) -> f64 {
    let (score, winrate) = losses(position);
    score
        .or(winrate.map(|value| value / 10.0))
        .unwrap_or_else(|| {
            sorted_candidates(position)
                .first()
                .map_or(0.0, |candidate| candidate.visits as f64 / 1_000_000.0)
        })
}

fn validate(position: &PositionInput) -> Result<(), Error> {
    for candidate in &position.candidates {
        if !candidate.winrate.is_finite()
            || !(0.0..=1.0).contains(&candidate.winrate)
            || candidate.score_lead.is_some_and(|score| !score.is_finite())
            || candidate.mv.color() != position.board.to_play()
        {
            return Err(Error::InvalidAnalysis);
        }
        candidate
            .mv
            .to_gtp(position.board.size())
            .map_err(|_| Error::InvalidCoordinate)?;
    }
    Ok(())
}

/// Formats only selected immutable snapshots, with no game information or user comments.
pub fn build_evidence(
    inputs: &[PositionInput],
    select_key_positions: bool,
    language: Language,
) -> Result<Evidence, Error> {
    let available: Vec<_> = inputs
        .iter()
        .filter(|position| !position.candidates.is_empty())
        .collect();
    if available.is_empty() {
        return Err(Error::NoAnalysis);
    }
    for position in &available {
        validate(position)?;
    }
    let analyzed = available.len();
    let positions = if select_key_positions {
        let mut ranked = available;
        ranked.sort_by(|left, right| {
            importance(right)
                .total_cmp(&importance(left))
                .then_with(|| left.move_number.cmp(&right.move_number))
        });
        let mut chosen: Vec<&PositionInput> = Vec::new();
        for position in ranked {
            if chosen
                .iter()
                .any(|other| other.move_number.abs_diff(position.move_number) < 3)
            {
                continue;
            }
            chosen.push(position);
            if chosen.len() == 5 {
                break;
            }
        }
        chosen.sort_by_key(|position| position.move_number);
        chosen
    } else {
        available
    };
    let selected = positions.len();
    let omitted = analyzed - selected;
    let mut evidence = Evidence {
        text: if language.is_chinese() {
            format!("已分析局面：{analyzed}；选中：{selected}；略去：{omitted}。\n")
        } else {
            format!("Analyzed positions: {analyzed}; selected: {selected}; omitted: {omitted}.\n")
        },
        positions: Vec::new(),
        analyzed,
        selected,
        omitted,
        allowed_coordinates: BTreeSet::new(),
    };
    for position in positions {
        evidence.positions.push(SelectedPosition {
            node_id: position.node_id,
            move_number: position.move_number,
        });
        evidence.text.push('\n');
        evidence.text.push_str(&format_position(
            position,
            language,
            &mut evidence.allowed_coordinates,
        )?);
        if evidence.text.chars().count() > MAX_PROMPT_CHARACTERS {
            return Err(Error::PromptTooLong);
        }
    }
    Ok(evidence)
}

fn format_position(
    position: &PositionInput,
    language: Language,
    allowed: &mut BTreeSet<String>,
) -> Result<String, Error> {
    let chinese = language.is_chinese();
    let size = position.board.size();
    let color = position.board.to_play();
    let color_text = board::color_name(color);
    let mut text = if chinese {
        format!(
            "第 {} 手后；轮到 {color_text}；根节点计算量={}。\n视角：胜率和目差均为轮到的一方 {color_text}；胜率单位 %，目差单位目。\n",
            position.move_number, position.root_visits
        )
    } else {
        format!(
            "After move {}; side to play: {color_text}; root visits={}.\nPerspective: winrate and score lead are for side to play {color_text}; winrate in %, score lead in points.\n",
            position.move_number, position.root_visits
        )
    };
    text.push_str(&format!(
        "{}: {:?}\n",
        if chinese {
            "重放规则"
        } else {
            "Replay rules"
        },
        position.board.rules()
    ));
    let candidates = sorted_candidates(position);
    let mut shown: Vec<_> = candidates.iter().copied().take(3).collect();
    if let Some(actual) = position.actual_move {
        let coordinate = actual.to_gtp(size).map_err(|_| Error::InvalidCoordinate)?;
        if matches!(actual, Move::Play(..)) {
            allowed.insert(coordinate.clone());
        }
        text.push_str(&format!(
            "{}：{} {coordinate}\n",
            if chinese {
                "实际下一手"
            } else {
                "Actual next move"
            },
            board::color_name(actual.color())
        ));
        if let Some(candidate) = candidates.iter().find(|candidate| candidate.mv == actual) {
            if !shown.iter().any(|shown| shown.mv == actual) {
                shown.push(candidate);
            }
        } else {
            text.push_str(if chinese {
                "实际着手不在候选中，损失未知。\n"
            } else {
                "The actual move is not in the candidates; loss is unknown.\n"
            });
        }
    } else {
        text.push_str(if chinese {
            "实际下一手：没有记录。\n"
        } else {
            "Actual next move: not available.\n"
        });
    }
    let (score_loss, winrate_loss) = losses(position);
    if let Some(loss) = score_loss {
        text.push_str(&format!(
            "{}={loss:.1}\n",
            if chinese {
                "实际着手相对首选的目差损失（目）"
            } else {
                "Actual move score loss versus top candidate (points)"
            }
        ));
    }
    if let Some(loss) = winrate_loss {
        text.push_str(&format!(
            "{}={loss:.1}\n",
            if chinese {
                "实际着手相对首选的胜率损失（百分点）"
            } else {
                "Actual move winrate loss versus top candidate (percentage points)"
            }
        ));
    }
    let mut relevant = HashSet::new();
    if let Some(Move::Play(_, point)) = position.actual_move {
        relevant.insert(point);
    }
    let mut lines = String::new();
    let mut actual_line = position.continuation.clone();
    if position.actual_move.is_some() && actual_line.first().copied() != position.actual_move {
        actual_line = position.actual_move.into_iter().collect();
    }
    if !actual_line.is_empty() {
        lines.push_str(if chinese {
            "实际后续本地重放："
        } else {
            "Recorded continuation, local replay: "
        });
        let (line, points) = board::replay(&position.board, &actual_line, 5, language, allowed)?;
        lines.push_str(&line);
        relevant.extend(points);
    }
    for candidate in shown {
        let coordinate = candidate
            .mv
            .to_gtp(size)
            .map_err(|_| Error::InvalidCoordinate)?;
        if let Move::Play(_, point) = candidate.mv {
            allowed.insert(coordinate.clone());
            relevant.insert(point);
        }
        let score =
            score(candidate, color).map_or_else(|| "unknown".into(), |value| format!("{value:.1}"));
        text.push_str(&format!(
            "{} #{}: {coordinate}, visits={}, winrate={:.1}%, scoreLead={score}\n",
            if chinese { "候选" } else { "Candidate" },
            u64::from(candidate.order) + 1,
            candidate.visits,
            winrate(candidate, color)
        ));
        let variation: &[Move] = if candidate.pv.first() == Some(&candidate.mv) {
            &candidate.pv
        } else {
            std::slice::from_ref(&candidate.mv)
        };
        lines.push_str(&format!(
            "{} {coordinate}: ",
            if chinese {
                "候选变化本地重放"
            } else {
                "Candidate local replay"
            }
        ));
        let (line, points) = board::replay(&position.board, variation, 12, language, allowed)?;
        lines.push_str(&line);
        relevant.extend(points);
    }
    text.push_str(&board::board_facts(
        &position.board,
        &relevant,
        language,
        allowed,
    )?);
    text.push_str(&lines);
    Ok(text)
}
