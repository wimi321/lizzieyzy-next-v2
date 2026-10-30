//! Fox transport cleanup and deterministic main-line extraction. Values retain
//! their original SGF escapes; Go rules and komi interpretation stay elsewhere.

use crate::FoxError;

const ROOT_ORDER: &[&str] = &[
    "GM", "FF", "CA", "AP", "ST", "RU", "SZ", "KM", "HA", "TM", "TC", "TT", "OT", "EV", "RO", "PC",
    "DT", "GN", "GC", "PB", "BR", "PW", "WR", "RE", "US", "SO", "CP", "AN", "ON", "BT", "WT", "PL",
    "AB", "AW", "AE", "RN", "RL",
];
const MAX_DEPTH: usize = 256;

#[derive(Clone, Debug)]
struct Property {
    name: String,
    values: Vec<String>,
}

#[derive(Default, Debug)]
struct Tree {
    nodes: Vec<Vec<Property>>,
    children: Vec<usize>,
    has_moves: bool,
}

#[derive(Clone, Debug)]
struct Move {
    color: char,
    value: String,
    node: usize,
}

/// Match the Java downloader's transport cleanup, root metadata selection,
/// preferred variation and move ordering without a second Go rules engine.
/// In particular, never rewrite KM: document owns its source-aware conversion.
pub(crate) fn normalize_sgf(input: &str) -> Result<String, FoxError> {
    let clean = sanitize(input);
    let trees = Parser::new(&clean).parse()?;
    let nodes = main_line(&trees);
    let root = nodes.first().ok_or_else(invalid)?;
    let leading = trees[0]
        .nodes
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, node)| node.iter().any(is_move))
        .map_or(trees[0].nodes.len(), |(index, _)| index);
    let mut root_properties = root_properties(&nodes[..leading]);
    // Java's metadata rebuild discards unknown data. Keep the selected line's
    // comments and unknown properties instead of losing them during download.
    root_properties.extend(root.iter().filter(|p| !is_move(p) && !is_root(p)).cloned());

    let mut ordered = compatible_prefix(
        moves(root, 0),
        nodes
            .iter()
            .skip(1)
            .flat_map(|node| moves(node, 0))
            .next()
            .map(|mv| mv.color),
    );
    for (index, node) in nodes.iter().enumerate().skip(1) {
        ordered.extend(moves(node, index));
    }
    let handicap = ordered
        .iter()
        .take_while(|mv| mv.color == 'B' && mv.value.len() == 2 && mv.value != "tt")
        .count();
    let skip = if handicap >= 2 {
        root_properties.retain(|p| p.name != "HA" && p.name != "AB");
        root_properties.push(Property {
            name: "HA".into(),
            values: vec![handicap.to_string()],
        });
        root_properties.push(Property {
            name: "AB".into(),
            values: ordered
                .iter()
                .take(handicap)
                .map(|mv| mv.value.clone())
                .collect(),
        });
        handicap
    } else {
        0
    };

    root_properties.sort_by_key(|p| {
        ROOT_ORDER
            .iter()
            .position(|&name| name == p.name)
            .unwrap_or(ROOT_ORDER.len())
    });

    let mut selected: Vec<Vec<Move>> = (0..nodes.len()).map(|_| Vec::new()).collect();
    let mut last = None;
    for mv in ordered.into_iter().skip(skip) {
        if last != Some(mv.color) {
            last = Some(mv.color);
            selected[mv.node].push(mv);
        }
    }

    let mut output = String::with_capacity(clean.len());
    output.push_str("(;");
    append_properties(&mut output, &root_properties);
    append_node(&mut output, &selected[0], &[]);
    for (index, node) in nodes.iter().enumerate().skip(1) {
        let extras: Vec<_> = node
            .iter()
            .filter(|p| !is_move(p) && (index >= leading || !is_root(p)))
            .cloned()
            .collect();
        append_node(&mut output, &selected[index], &extras);
    }
    output.push(')');
    Ok(output)
}

fn invalid() -> FoxError {
    FoxError::InvalidResponse("chess")
}

fn sanitize(input: &str) -> String {
    // Java replace removes every BOM before interpreting escapes, including a
    // BOM embedded in a value. Java String.trim strips chars <= U+0020 only.
    let without_bom: String = input.chars().filter(|&c| c != '\u{feff}').collect();
    let text = without_bom.trim_matches(|c: char| c <= '\u{20}');
    let mut output = String::with_capacity(text.len());
    let mut inside = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if inside {
            output.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    output.push(next);
                }
            } else if c == ']' {
                inside = false;
            }
        } else if c != '\\' {
            output.push(c);
            if c == '[' {
                inside = true;
            }
        }
    }
    output
}

fn is_move(property: &Property) -> bool {
    matches!(property.name.as_str(), "B" | "W")
}

fn is_root(property: &Property) -> bool {
    ROOT_ORDER.contains(&property.name.as_str())
}

fn moves(properties: &[Property], node: usize) -> Vec<Move> {
    properties
        .iter()
        .filter(|p| is_move(p))
        .flat_map(|p| {
            let color = if p.name == "B" { 'B' } else { 'W' };
            p.values.iter().map(move |value| Move {
                color,
                value: value.clone(),
                node,
            })
        })
        .collect()
}

fn main_line(trees: &[Tree]) -> Vec<&Vec<Property>> {
    let mut result = Vec::new();
    let mut current = Some(0);
    while let Some(index) = current {
        let tree = &trees[index];
        result.extend(tree.nodes.iter());
        current = tree
            .children
            .iter()
            .copied()
            .find(|&child| trees[child].has_moves);
    }
    result
}

fn compatible_prefix(mut moves: Vec<Move>, next: Option<char>) -> Vec<Move> {
    let mut alternating = 0;
    let mut compatible = 0;
    let mut last = None;
    for mv in &moves {
        if last == Some(mv.color) {
            break;
        }
        last = Some(mv.color);
        alternating += 1;
        if next != Some(mv.color) {
            compatible = alternating;
        }
    }
    moves.truncate(if compatible > 0 {
        compatible
    } else {
        alternating
    });
    moves
}

fn root_properties(nodes: &[&Vec<Property>]) -> Vec<Property> {
    let mut merged: Vec<Property> = Vec::new();
    for property in nodes
        .iter()
        .flat_map(|node| node.iter())
        .filter(|p| is_root(p))
    {
        if let Some(existing) = merged.iter_mut().find(|p| p.name == property.name) {
            if matches!(property.name.as_str(), "AB" | "AW" | "AE") {
                existing.values.extend(property.values.clone());
            } else if property.values.iter().any(|value| !value.trim().is_empty()) {
                *existing = property.clone();
            }
        } else {
            merged.push(property.clone());
        }
    }
    for (name, value) in [("GM", "1"), ("FF", "4"), ("CA", "UTF-8"), ("SZ", "19")] {
        if !merged.iter().any(|p| p.name == name) {
            merged.push(Property {
                name: name.into(),
                values: vec![value.into()],
            });
        }
    }
    merged.sort_by_key(|p| ROOT_ORDER.iter().position(|&name| name == p.name));
    merged
}

fn append_properties(output: &mut String, properties: &[Property]) {
    for property in properties {
        output.push_str(&property.name);
        for value in &property.values {
            output.push('[');
            output.push_str(value);
            output.push(']');
        }
    }
}

fn append_node(output: &mut String, moves: &[Move], extras: &[Property]) {
    if moves.is_empty() {
        if !extras.is_empty() {
            output.push(';');
            append_properties(output, extras);
        }
        return;
    }
    for (index, mv) in moves.iter().enumerate() {
        output.push(';');
        output.push(mv.color);
        output.push('[');
        output.push_str(&mv.value);
        output.push(']');
        if index == 0 {
            append_properties(output, extras);
        }
    }
}

/// Flat arena parser: no recursive parsing or recursive drop for remote trees.
/// The depth limit matches the document's SGF parser and bounds empty nesting.
struct Parser<'a> {
    text: &'a str,
    index: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, index: 0 }
    }

    fn parse(mut self) -> Result<Vec<Tree>, FoxError> {
        let mut trees: Vec<Tree> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        loop {
            self.whitespace();
            let Some(c) = self.peek() else {
                break;
            };
            match c {
                '(' => {
                    if stack.len() >= MAX_DEPTH {
                        return Err(invalid());
                    }
                    let index = trees.len();
                    if let Some(&parent) = stack.last() {
                        if trees[parent].nodes.is_empty() {
                            return Err(invalid());
                        }
                        trees[parent].children.push(index);
                    } else if !trees.is_empty() {
                        // A Fox game response must be a single complete game.
                        return Err(invalid());
                    }
                    trees.push(Tree::default());
                    stack.push(index);
                    self.index += 1;
                }
                ';' => {
                    let &index = stack.last().ok_or_else(invalid)?;
                    if !trees[index].children.is_empty() {
                        return Err(invalid());
                    }
                    trees[index].nodes.push(self.node()?);
                }
                ')' => {
                    let index = stack.pop().ok_or_else(invalid)?;
                    if trees[index].nodes.is_empty() {
                        return Err(invalid());
                    }
                    trees[index].has_moves = trees[index].nodes.iter().flatten().any(is_move)
                        || trees[index]
                            .children
                            .iter()
                            .any(|&child| trees[child].has_moves);
                    self.index += 1;
                }
                _ => return Err(invalid()),
            }
        }
        if !stack.is_empty() || trees.is_empty() {
            return Err(invalid());
        }
        Ok(trees)
    }

    fn node(&mut self) -> Result<Vec<Property>, FoxError> {
        self.index += 1;
        let mut properties = Vec::new();
        loop {
            self.whitespace();
            match self.peek() {
                Some(';' | '(' | ')') | None => return Ok(properties),
                _ => {}
            }
            let start = self.index;
            while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                self.index += 1;
            }
            if start == self.index {
                return Err(invalid());
            }
            let name = self.text[start..self.index].to_owned();
            self.whitespace();
            let mut values = Vec::new();
            while self.peek() == Some('[') {
                values.push(self.value()?);
                self.whitespace();
            }
            if values.is_empty() {
                return Err(invalid());
            }
            properties.push(Property { name, values });
        }
    }

    fn value(&mut self) -> Result<String, FoxError> {
        self.index += 1;
        let start = self.index;
        loop {
            match self.peek().ok_or_else(invalid)? {
                ']' => {
                    let value = self.text[start..self.index].to_owned();
                    self.index += 1;
                    return Ok(value);
                }
                '\\' => {
                    self.index += 1;
                    self.index += self.peek().ok_or_else(invalid)?.len_utf8();
                }
                c => self.index += c.len_utf8(),
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.index..].chars().next()
    }

    fn whitespace(&mut self) {
        while let Some(c) = self.peek() {
            // Java Character.isWhitespace excludes the three nonbreaking spaces.
            if (c.is_whitespace() && !matches!(c, '\u{a0}' | '\u{2007}' | '\u{202f}'))
                || matches!(c, '\u{1c}'..='\u{1f}')
            {
                self.index += c.len_utf8();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests;
