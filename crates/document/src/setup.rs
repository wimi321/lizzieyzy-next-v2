//! Starting-position editing is restricted to a root snapshot. Converting a
//! played game is an explicit document replacement, never a silent tree loss.
use go_core::{BoardSize, Color, Point};
use sgf::{GameTree, Node, Property};

use crate::edit::{Change, Edit};
use crate::{DocError, GameDocument, metadata, semantics};

struct Setup {
    size: BoardSize,
    stones: Vec<Option<Color>>,
    player: Color,
}

impl GameDocument {
    fn setup_position(&self) -> Result<Setup, DocError> {
        let root = self.node(self.root)?;
        if self.current != self.root
            || !root.children().is_empty()
            || root
                .properties()
                .iter()
                .any(|p| matches!(p.identifier.as_str(), "B" | "W"))
        {
            return Err(DocError::SetupRequiresRoot);
        }
        let meta = metadata::root_metadata(self.root, root.properties());
        let player = metadata::player(self.root, root.properties(), &mut Vec::new()).unwrap_or(
            if meta.handicap >= 2 {
                Color::White
            } else {
                Color::Black
            },
        );
        let mut stones = vec![None; meta.size.area()];
        let mut seen = vec![false; meta.size.area()];
        for p in root.properties() {
            let color = match p.identifier.as_str() {
                "AB" => Some(Color::Black),
                "AW" => Some(Color::White),
                "AE" => None,
                _ => continue,
            };
            for value in &p.values {
                for point in semantics::setup_points(self.root, &p.identifier, value, meta.size)? {
                    let index =
                        usize::from(point.y) * usize::from(meta.size.width) + usize::from(point.x);
                    if seen[index] {
                        return Err(DocError::InvalidProperty {
                            node: self.root,
                            property: p.identifier.clone(),
                            value: value.clone(),
                        });
                    }
                    seen[index] = true;
                    stones[index] = color;
                }
            }
        }
        Ok(Setup {
            size: meta.size,
            stones,
            player,
        })
    }

    pub(crate) fn updated_setup_stone(
        &self,
        point: Point,
        color: Option<Color>,
    ) -> Result<Vec<Property>, DocError> {
        let position = self.setup_position()?;
        let size = position.size;
        point.to_sgf(size).map_err(|_| DocError::InvalidProperty {
            node: self.root,
            property: "setup".into(),
            value: format!("{point:?}"),
        })?;
        if position.stones[usize::from(point.y) * usize::from(size.width) + usize::from(point.x)]
            == color
        {
            return Ok(self.node(self.root)?.properties().to_vec());
        }
        let mut stones = position.stones.clone();
        stones[usize::from(point.y) * usize::from(size.width) + usize::from(point.x)] = color;
        // Like Java setupPlaceStone this does not capture or reject zero-liberty
        // groups. Replay still rejects them; erasing/clearing can repair them.
        materialize(
            self.node(self.root)?.properties(),
            position.size,
            position.player,
            &stones,
        )
    }

    pub(crate) fn cleared_setup(&self) -> Result<Vec<Property>, DocError> {
        let position = self.setup_position()?;
        if position.stones.iter().all(Option::is_none) {
            return Ok(self.node(self.root)?.properties().to_vec());
        }
        materialize(
            self.node(self.root)?.properties(),
            position.size,
            position.player,
            &vec![None; position.size.area()],
        )
    }

    pub(crate) fn updated_first_player(&self, color: Color) -> Result<Vec<Property>, DocError> {
        let position = self.setup_position()?;
        let mut properties = self.node(self.root)?.properties().to_vec();
        if position.player != color {
            replace_player(&mut properties, color);
        }
        Ok(properties)
    }

    /// Java's conversion discards the entire old move tree and creates a new
    /// root snapshot; the application handles it like new_game, with no undo
    /// link to the replaced document. Non-setup root properties are retained.
    pub fn to_start_position(&self) -> Result<Self, DocError> {
        let position = self.position_at(self.current)?;
        let properties = materialize(
            self.node(self.root)?.properties(),
            position.board.size(),
            position.board.to_play(),
            position.board.stones(),
        )?;
        let converted = Self::from_sgf(&GameTree {
            sequence: vec![Node { properties }],
            children: Vec::new(),
        })?;
        converted.position_at(converted.root)?;
        Ok(converted)
    }

    pub(crate) fn prepare_toggle_to_play(&mut self) -> Result<Option<Edit>, DocError> {
        let position = self.position_at(self.current)?;
        let color = position.board.to_play().opponent();
        let mut properties = self.node(self.current)?.properties().to_vec();
        if properties
            .iter()
            .any(|p| matches!(p.identifier.as_str(), "B" | "W"))
        {
            // PL is applied before a move in an SGF node. A separate empty child
            // is necessary to persist the changed player AFTER the current move.
            let mut properties = Vec::new();
            replace_player(&mut properties, color);
            let id = self.allocate(Some(self.current), properties)?;
            Ok(Some(Edit {
                before: self.current,
                after: id,
                change: Change::Add {
                    id,
                    parent: self.current,
                },
            }))
        } else {
            replace_player(&mut properties, color);
            self.property_edit(self.current, properties)
        }
    }
}

fn replace_player(properties: &mut Vec<Property>, color: Color) {
    let first = properties
        .iter()
        .position(|p| p.identifier == "PL")
        .unwrap_or(properties.len());
    properties.retain(|p| p.identifier != "PL");
    properties.insert(
        first,
        Property {
            identifier: "PL".into(),
            values: vec![if color == Color::Black { "B" } else { "W" }.into()],
        },
    );
}

fn materialize(
    original: &[Property],
    size: BoardSize,
    player: Color,
    stones: &[Option<Color>],
) -> Result<Vec<Property>, DocError> {
    let mut properties: Vec<_> = original
        .iter()
        .filter(|p| !matches!(p.identifier.as_str(), "AB" | "AW" | "AE" | "PL" | "B" | "W"))
        .cloned()
        .collect();
    for (name, color) in [("AB", Color::Black), ("AW", Color::White)] {
        let mut values = Vec::new();
        for (index, stone) in stones.iter().enumerate() {
            if *stone == Some(color) {
                let point = Point {
                    x: (index % usize::from(size.width)) as u8,
                    y: (index / usize::from(size.width)) as u8,
                };
                values.push(point.to_sgf(size).map_err(|_| DocError::InvalidProperty {
                    node: crate::NodeId(0),
                    property: name.into(),
                    value: format!("{point:?}"),
                })?);
            }
        }
        if !values.is_empty() {
            properties.push(Property {
                identifier: name.into(),
                values,
            });
        }
    }
    replace_player(&mut properties, player);
    Ok(properties)
}

#[cfg(test)]
mod tests;
