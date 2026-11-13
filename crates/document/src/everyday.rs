//! Detached clean exports and reversible, whole-tree geometric edits.

use go_core::{BoardSize, Color, Move, Point};
use sgf::{GameTree, Node, Property};

use crate::edit::{Change, Edit};
use crate::{DocError, GameDocument, NodeId, metadata};

mod payload;

/// Java Board.addMoveToList's four transforms, in top-left-origin coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardTransform {
    RotateRight,
    RotateLeft,
    FlipHorizontal,
    FlipVertical,
}

#[derive(Debug, Clone)]
pub(crate) struct PropertyPatch {
    pub id: NodeId,
    pub before: Vec<Property>,
    pub after: Vec<Property>,
}

impl BoardTransform {
    pub(super) fn point(self, point: Point, size: BoardSize) -> Point {
        let last = size.width - 1;
        match self {
            Self::RotateRight => Point {
                x: last - point.y,
                y: point.x,
            },
            Self::RotateLeft => Point {
                x: point.y,
                y: last - point.x,
            },
            Self::FlipHorizontal => Point {
                x: last - point.x,
                y: point.y,
            },
            Self::FlipVertical => Point {
                x: point.x,
                y: size.height - 1 - point.y,
            },
        }
    }
}

impl GameDocument {
    /// Raw export removes both Java analysis slots and, optionally, comments.
    /// Current-branch export follows the root-to-focus path, then the first
    /// continuation to its leaf, matching Java's selectedBranch traversal.
    /// The document, history, focus, and revisions are never modified.
    pub fn to_clean_sgf(&self, keep_comments: bool, current_branch: bool) -> GameTree {
        let clean = |properties: &[Property]| {
            properties
                .iter()
                .filter(|property| {
                    !matches!(
                        property.identifier.as_str(),
                        "LZ" | "LZOP" | "LZ2" | "LZOP2" | "DZ" | "FIT"
                    ) && (keep_comments || property.identifier != "C")
                })
                .cloned()
                .collect()
        };
        if current_branch {
            let mut path = Vec::new();
            let mut next = Some(self.current);
            while let Some(id) = next {
                path.push(id);
                next = self.arena[id.0 as usize].parent;
            }
            path.reverse();
            next = self.arena[self.current.0 as usize]
                .children
                .first()
                .copied();
            while let Some(id) = next {
                path.push(id);
                next = self.arena[id.0 as usize].children.first().copied();
            }
            GameTree {
                sequence: path
                    .into_iter()
                    .map(|id| Node {
                        properties: clean(&self.arena[id.0 as usize].properties),
                    })
                    .collect(),
                children: Vec::new(),
            }
        } else {
            let mut tree = self.to_sgf();
            let mut pending = vec![&mut tree];
            while let Some(tree) = pending.pop() {
                for node in &mut tree.sequence {
                    node.properties = clean(&node.properties);
                }
                pending.extend(&mut tree.children);
            }
            tree
        }
    }

    pub(crate) fn prepare_transform(
        &self,
        transform: BoardTransform,
    ) -> Result<Option<Edit>, DocError> {
        let size = metadata::root_metadata(self.root, self.node(self.root)?.properties()).size;
        if size.width != size.height
            && matches!(
                transform,
                BoardTransform::RotateRight | BoardTransform::RotateLeft
            )
        {
            return Err(DocError::TransformRequiresSquare(size));
        }
        let mut patches = Vec::new();
        for node in self.arena.iter().filter(|node| node.live) {
            let mut after = node.properties.clone();
            for property in &mut after {
                for value in &mut property.values {
                    let updated = transform_value(&property.identifier, value, size, transform)
                        .map_err(|()| DocError::InvalidProperty {
                            node: node.id,
                            property: property.identifier.clone(),
                            value: value.clone(),
                        })?;
                    *value = updated;
                }
            }
            if after != node.properties {
                patches.push(PropertyPatch {
                    id: node.id,
                    before: node.properties.clone(),
                    after,
                });
            }
        }
        Ok((!patches.is_empty()).then_some(Edit {
            before: self.current,
            after: self.current,
            change: Change::PropertiesMany(patches),
        }))
    }
}

fn transform_value(
    name: &str,
    value: &str,
    size: BoardSize,
    transform: BoardTransform,
) -> Result<String, ()> {
    match name {
        "B" | "W" => match Move::from_sgf(Color::Black, value, size).map_err(|_| ())? {
            Move::Pass(_) => Ok(value.into()),
            Move::Play(_, point) => transform.point(point, size).to_sgf(size).map_err(|_| ()),
        },
        "AB" | "AW" | "AE" | "TR" | "SQ" | "CR" | "MA" | "SL" | "TB" | "TW" | "DD" | "VW" | "M"
        | "L" => {
            // Java uses DD[true] for a non-spatial dummy node as well.
            if (value.is_empty() && matches!(name, "DD" | "VW"))
                || (name == "DD" && value == "true")
            {
                return Ok(value.into());
            }
            if let Some((first, last)) = value.split_once(':') {
                let first = Point::from_sgf(first, size).map_err(|_| ())?;
                let last = Point::from_sgf(last, size).map_err(|_| ())?;
                if first.x > last.x || first.y > last.y {
                    return Err(());
                }
                let first = transform.point(first, size);
                let last = transform.point(last, size);
                let start = Point {
                    x: first.x.min(last.x),
                    y: first.y.min(last.y),
                };
                let end = Point {
                    x: first.x.max(last.x),
                    y: first.y.max(last.y),
                };
                Ok(format!(
                    "{}:{}",
                    start.to_sgf(size).map_err(|_| ())?,
                    end.to_sgf(size).map_err(|_| ())?
                ))
            } else {
                sgf_point(value, size, transform)
            }
        }
        "LB" => {
            let (point, label) = value.split_once(':').ok_or(())?;
            Ok(format!("{}:{label}", sgf_point(point, size, transform)?))
        }
        "AR" | "LN" => {
            let (first, last) = value.split_once(':').ok_or(())?;
            Ok(format!(
                "{}:{}",
                sgf_point(first, size, transform)?,
                sgf_point(last, size, transform)?
            ))
        }
        "LZ" | "LZOP" | "LZ2" | "LZOP2" => payload::transform(value, size, transform),
        _ => Ok(value.into()),
    }
}

fn sgf_point(value: &str, size: BoardSize, transform: BoardTransform) -> Result<String, ()> {
    let point = Point::from_sgf(value, size).map_err(|_| ())?;
    transform.point(point, size).to_sgf(size).map_err(|_| ())
}

#[cfg(test)]
mod tests;
