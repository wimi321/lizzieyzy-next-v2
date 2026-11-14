use std::collections::{BTreeSet, HashSet};

use go_core::{BoardSize, Color, Move, Point, Position};

use crate::{Error, Language};

#[derive(Debug)]
struct Group {
    color: Color,
    stones: Vec<Point>,
    liberties: Vec<Point>,
    liberty_count: usize,
}

pub(crate) fn color_name(color: Color) -> &'static str {
    match color {
        Color::Black => "B",
        Color::White => "W",
    }
}

fn neighbors(point: Point, size: BoardSize) -> impl Iterator<Item = Point> {
    [
        point.x.checked_sub(1).map(|x| Point { x, ..point }),
        (point.x + 1 < size.width).then_some(Point {
            x: point.x + 1,
            ..point
        }),
        point.y.checked_sub(1).map(|y| Point { y, ..point }),
        (point.y + 1 < size.height).then_some(Point {
            y: point.y + 1,
            ..point
        }),
    ]
    .into_iter()
    .flatten()
}

fn groups(board: &Position) -> Vec<Group> {
    let size = board.size();
    let mut visited = HashSet::new();
    let mut result = Vec::new();
    for y in 0..size.height {
        for x in 0..size.width {
            let origin = Point { x, y };
            let Some(color) = board.stone_at(origin) else {
                continue;
            };
            if !visited.insert(origin) {
                continue;
            }
            let mut stones = vec![origin];
            let mut liberties = HashSet::new();
            let mut cursor = 0;
            while cursor < stones.len() {
                for neighbor in neighbors(stones[cursor], size) {
                    match board.stone_at(neighbor) {
                        None => {
                            liberties.insert(neighbor);
                        }
                        Some(other) if other == color && visited.insert(neighbor) => {
                            stones.push(neighbor);
                        }
                        _ => {}
                    }
                }
                cursor += 1;
            }
            stones.sort_unstable_by_key(|point| (point.y, point.x));
            let mut liberties: Vec<_> = liberties.into_iter().collect();
            liberties.sort_unstable_by_key(|point| (point.y, point.x));
            result.push(Group {
                color,
                stones,
                liberties,
                liberty_count: board.liberties_at(origin).unwrap_or(0),
            });
        }
    }
    result
}

fn coordinates(points: &[Point], size: BoardSize) -> Result<Vec<String>, Error> {
    points
        .iter()
        .map(|point| point.to_gtp(size).map_err(|_| Error::InvalidCoordinate))
        .collect()
}

fn group_line(group: &Group, size: BoardSize, language: Language) -> Result<String, Error> {
    let stones = coordinates(&group.stones, size)?.join(" ");
    let liberties = coordinates(&group.liberties, size)?.join(" ");
    Ok(if language.is_chinese() {
        format!(
            "{} 子=[{stones}] 气={} [{liberties}]",
            color_name(group.color),
            group.liberty_count
        )
    } else {
        format!(
            "{} stones=[{stones}] liberties={} [{liberties}]",
            color_name(group.color),
            group.liberty_count
        )
    })
}

pub(crate) fn board_facts(
    board: &Position,
    relevant: &HashSet<Point>,
    language: Language,
    allowed: &mut BTreeSet<String>,
) -> Result<String, Error> {
    let size = board.size();
    let mut text = if language.is_chinese() {
        format!(
            "棋盘：{}x{}。块和气是几何事实，不代表死活或先手。\n",
            size.width, size.height
        )
    } else {
        format!(
            "Board: {}x{}. Groups and liberties are geometry, not life/death or sente judgments.\n",
            size.width, size.height
        )
    };
    let all = groups(board);
    if all.is_empty() {
        text.push_str(if language.is_chinese() {
            "空盘。\n"
        } else {
            "Empty board.\n"
        });
    }
    for group in all {
        // Only groups involved in supplied moves are named for commentary. In particular,
        // merely appearing in the full board inventory does not whitelist a remote stone.
        let named = group.stones.iter().any(|point| {
            relevant.contains(point) || neighbors(*point, size).any(|p| relevant.contains(&p))
        });
        if named {
            allowed.extend(coordinates(&group.stones, size)?);
            text.push_str(if language.is_chinese() {
                "关联棋块："
            } else {
                "Named group: "
            });
        }
        text.push_str(&group_line(&group, size, language)?);
        text.push('\n');
        if group.liberty_count <= 2 {
            text.push_str(if language.is_chinese() {
                "少气棋块："
            } else {
                "Low-liberty group: "
            });
            text.push_str(&group_line(&group, size, language)?);
            text.push('\n');
        }
    }
    Ok(text)
}

pub(crate) fn replay(
    initial: &Position,
    moves: &[Move],
    limit: usize,
    language: Language,
    allowed: &mut BTreeSet<String>,
) -> Result<(String, HashSet<Point>), Error> {
    let mut board = initial.clone();
    let mut text = String::new();
    let mut relevant = HashSet::new();
    for (index, mv) in moves.iter().take(limit).enumerate() {
        let Ok(coordinate) = mv.to_gtp(board.size()) else {
            text.push_str(if language.is_chinese() {
                "停止：坐标越界，不解释剩余变化。"
            } else {
                "STOPPED: out-of-bounds coordinate; do not explain the remaining line."
            });
            break;
        };
        let failure = if mv.color() != board.to_play() {
            Some("wrong player".to_owned())
        } else {
            match board.play(*mv) {
                Ok(outcome) => {
                    if let Move::Play(_, point) = mv {
                        allowed.insert(coordinate.clone());
                        relevant.insert(*point);
                    }
                    text.push_str(&format!(
                        "{}.{} {coordinate}",
                        index + 1,
                        color_name(mv.color())
                    ));
                    if !outcome.captured.is_empty() {
                        let captures = coordinates(&outcome.captured, board.size())?;
                        allowed.extend(captures.iter().cloned());
                        relevant.extend(outcome.captured.iter().copied());
                        text.push_str(&format!(
                            " {}=[{}]",
                            if language.is_chinese() {
                                "提子"
                            } else {
                                "captures"
                            },
                            captures.join(" ")
                        ));
                    }
                    if !outcome.self_captured.is_empty() {
                        let captures = coordinates(&outcome.self_captured, board.size())?;
                        allowed.extend(captures.iter().cloned());
                        text.push_str(&format!(
                            " {}=[{}]",
                            if language.is_chinese() {
                                "自提"
                            } else {
                                "self-captures"
                            },
                            captures.join(" ")
                        ));
                    }
                    text.push_str("; ");
                    None
                }
                Err(error) => Some(error.to_string()),
            }
        };
        if let Some(reason) = failure {
            text.push_str(if language.is_chinese() {
                "停止：不合法着手；不解释剩余变化。"
            } else {
                "STOPPED: illegal move; do not explain the remaining line. "
            });
            text.push_str(&reason);
            break;
        }
    }
    text.push('\n');
    for group in groups(&board)
        .into_iter()
        .filter(|group| group.liberty_count <= 2)
    {
        if group.stones.iter().any(|point| {
            relevant.contains(point)
                || neighbors(*point, board.size()).any(|p| relevant.contains(&p))
        }) {
            allowed.extend(coordinates(&group.stones, board.size())?);
        }
        text.push_str(if language.is_chinese() {
            "重放后少气棋块："
        } else {
            "After replay, low-liberty group: "
        });
        text.push_str(&group_line(&group, board.size(), language)?);
        text.push('\n');
    }
    Ok((text, relevant))
}
