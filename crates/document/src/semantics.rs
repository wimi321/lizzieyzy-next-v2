use go_core::{BoardSize, Color, Move, Point, Position};
use sgf::Property;

use crate::{Captures, DocError, DocumentWarning, GamePosition, NodeId, metadata};

/// Replay preserves the recorded move colors and skips only ko checking.
pub(crate) fn replay<'a>(
    nodes: impl IntoIterator<Item = (NodeId, &'a [Property])>,
) -> Result<GamePosition, DocError> {
    let mut nodes = nodes.into_iter();
    let Some((root, properties)) = nodes.next() else {
        return Err(DocError::EmptyTree);
    };
    let metadata::RootMetadata {
        size,
        rules,
        rules_raw,
        handicap,
        komi,
        ..
    } = metadata::root_metadata(root, properties);
    let initial_player = if handicap >= 2 {
        Color::White
    } else {
        Color::Black
    };
    let board = Position::with_setup(size, rules.rules(), initial_player, [])
        .map_err(|reason| DocError::InvalidSetup { node: root, reason })?;
    let mut position = GamePosition {
        board,
        moves: Vec::new(),
        initial_stones: Vec::new(),
        initial_player,
        analysis_moves: Vec::new(),
        rules,
        rules_raw,
        komi,
        handicap,
        captured_by: Captures::default(),
    };
    apply_node(&mut position, root, properties)?;
    for (node, properties) in nodes {
        apply_node(&mut position, node, properties)?;
    }
    Ok(position)
}

/// Diagnose all live nodes independently of whether any move can be replayed.
pub(crate) fn warnings<'a>(
    root: NodeId,
    nodes: impl IntoIterator<Item = (NodeId, &'a [Property])>,
) -> Vec<DocumentWarning> {
    let mut warnings = Vec::new();
    for (node, properties) in nodes {
        if node == root {
            warnings.extend(metadata::root_metadata(node, properties).warnings);
        } else {
            metadata::non_root_warnings(node, properties, &mut warnings);
        }
        metadata::player(node, properties, &mut warnings);
    }
    warnings
}

fn apply_node(
    position: &mut GamePosition,
    node: NodeId,
    properties: &[Property],
) -> Result<(), DocError> {
    let size = position.board.size();
    let player =
        metadata::player(node, properties, &mut Vec::new()).unwrap_or(position.board.to_play());
    let mut stones = position.board.stones().to_vec();
    let mut seen = vec![false; size.area()];
    let mut has_setup = false;
    // Setup is one edit of the previous board, never a sequence of captures.
    // A point in two different setup lists is ambiguous, so reject it.
    for (name, color) in [
        ("AE", None),
        ("AB", Some(Color::Black)),
        ("AW", Some(Color::White)),
    ] {
        for property in properties.iter().filter(|p| p.identifier == name) {
            has_setup = true;
            if property.values.is_empty() {
                return Err(invalid(node, name, ""));
            }
            for value in &property.values {
                for point in setup_points(node, name, value, size)? {
                    let index =
                        usize::from(point.y) * usize::from(size.width) + usize::from(point.x);
                    if seen[index] {
                        return Err(invalid(node, name, value));
                    }
                    seen[index] = true;
                    stones[index] = color;
                }
            }
        }
    }
    if has_setup || player != position.board.to_play() {
        let initial_stones = stones_with_points(&stones, size);
        position.board = Position::with_setup(
            size,
            position.rules.rules(),
            player,
            initial_stones.iter().copied(),
        )
        .map_err(|reason| DocError::InvalidSetup { node, reason })?;
        position.initial_stones = initial_stones;
        position.initial_player = player;
        position.analysis_moves.clear();
    }

    let black = single(node, properties, "B")?;
    let white = single(node, properties, "W")?;
    let (color, value, name) = match (black, white) {
        (Some(value), None) => (Color::Black, value, "B"),
        (None, Some(value)) => (Color::White, value, "W"),
        (None, None) => return Ok(()),
        (Some(_), Some(_)) => return Err(invalid(node, "B/W", "both move colors in one node")),
    };
    let mv = Move::from_sgf(color, value, size).map_err(|_| invalid(node, name, value))?;
    let outcome = position
        .board
        .play_unchecked_ko(mv)
        .map_err(|reason| DocError::IllegalMove { node, reason })?;
    match color {
        Color::Black => {
            position.captured_by.black += outcome.captured.len();
            position.captured_by.white += outcome.self_captured.len();
        }
        Color::White => {
            position.captured_by.white += outcome.captured.len();
            position.captured_by.black += outcome.self_captured.len();
        }
    }
    position.moves.push(mv);
    position.analysis_moves.push(mv);
    Ok(())
}

fn stones_with_points(stones: &[Option<Color>], size: BoardSize) -> Vec<(Color, Point)> {
    stones
        .iter()
        .enumerate()
        .filter_map(|(index, color)| {
            color.map(|color| {
                (
                    color,
                    Point {
                        x: (index % usize::from(size.width)) as u8,
                        y: (index / usize::from(size.width)) as u8,
                    },
                )
            })
        })
        .collect()
}

fn setup_points(
    node: NodeId,
    name: &str,
    value: &str,
    size: BoardSize,
) -> Result<Vec<Point>, DocError> {
    if let Some((first, last)) = value.split_once(':') {
        let first = Point::from_sgf(first, size).map_err(|_| invalid(node, name, value))?;
        let last = Point::from_sgf(last, size).map_err(|_| invalid(node, name, value))?;
        if first.x > last.x || first.y > last.y {
            return Err(invalid(node, name, value));
        }
        Ok((first.y..=last.y)
            .flat_map(|y| (first.x..=last.x).map(move |x| Point { x, y }))
            .collect())
    } else {
        Point::from_sgf(value, size)
            .map(|point| vec![point])
            .map_err(|_| invalid(node, name, value))
    }
}

fn single<'a>(
    node: NodeId,
    properties: &'a [Property],
    name: &str,
) -> Result<Option<&'a str>, DocError> {
    let mut matching = properties.iter().filter(|p| p.identifier == name);
    let Some(property) = matching.next() else {
        return Ok(None);
    };
    if property.values.len() != 1 || matching.next().is_some() {
        return Err(invalid(node, name, &property.values.join("][")));
    }
    Ok(property.values.first().map(String::as_str))
}

fn invalid(node: NodeId, property: &str, value: &str) -> DocError {
    DocError::InvalidProperty {
        node,
        property: property.to_owned(),
        value: value.to_owned(),
    }
}

#[cfg(test)]
mod tests;
