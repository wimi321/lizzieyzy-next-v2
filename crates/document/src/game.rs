//! Explicit game creation and root metadata edits. Imported metadata remains
//! tolerant; user commands validate before changing any content.
use go_core::{BoardSize, Point, RulePreset};
use sgf::Property;

use crate::{DocError, GameDocument, NodeId, metadata};

#[derive(Debug, Clone, PartialEq)]
pub struct NewGameOptions {
    pub width: u8,
    pub height: u8,
    pub komi: f64,
    /// Zero means an even game. Fixed handicap accepts 2..=9.
    pub handicap: u8,
    pub rules: String,
    pub black_name: Option<String>,
    pub black_rank: Option<String>,
    pub white_name: Option<String>,
    pub white_rank: Option<String>,
}

impl Default for NewGameOptions {
    fn default() -> Self {
        Self {
            width: 19,
            height: 19,
            komi: 7.5,
            handicap: 0,
            rules: "chinese".into(),
            black_name: None,
            black_rank: None,
            white_name: None,
            white_rank: None,
        }
    }
}

/// Complete editable metadata, rather than a partial patch. None removes a
/// text property; Some("") retains an explicitly empty value.
#[derive(Debug, Clone, PartialEq)]
pub struct GameInfoEdit {
    pub black_name: Option<String>,
    pub black_rank: Option<String>,
    pub white_name: Option<String>,
    pub white_rank: Option<String>,
    pub komi: f64,
    pub rules: String,
    pub result: Option<String>,
    pub date: Option<String>,
    pub event: Option<String>,
}

impl GameDocument {
    pub fn new_game(options: NewGameOptions) -> Result<Self, DocError> {
        let size = BoardSize::new(options.width, options.height)?;
        let points = fixed_handicap(size, options.handicap)?;
        let mut document = Self::new();
        let size_text = if size.width == size.height {
            size.width.to_string()
        } else {
            format!("{}:{}", size.width, size.height)
        };
        document.arena[0].properties = vec![
            property("GM", "1".into()),
            property("FF", "4".into()),
            property("SZ", size_text),
        ];
        let mut properties = document.game_info_properties(
            GameInfoEdit {
                black_name: options.black_name,
                black_rank: options.black_rank,
                white_name: options.white_name,
                white_rank: options.white_rank,
                komi: options.komi,
                rules: options.rules,
                result: None,
                date: None,
                event: None,
            },
            false,
        )?;
        properties.push(property("HA", options.handicap.to_string()));
        if !points.is_empty() {
            let values = points
                .into_iter()
                .map(|point| {
                    point.to_sgf(size).map_err(|_| DocError::InvalidProperty {
                        node: document.root,
                        property: "AB".into(),
                        value: format!("{point:?}"),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            properties.push(Property {
                identifier: "AB".into(),
                values,
            });
            properties.push(property("PL", "W".into()));
        }
        document.arena[0].properties = properties;
        document.position_at(document.root)?;
        Ok(document)
    }

    pub(crate) fn updated_game_info(&self, info: GameInfoEdit) -> Result<Vec<Property>, DocError> {
        self.game_info_properties(info, true)
    }

    fn game_info_properties(
        &self,
        info: GameInfoEdit,
        preserve_unchanged: bool,
    ) -> Result<Vec<Property>, DocError> {
        let root = self.node(self.root)?;
        let size = metadata::root_metadata(self.root, root.properties()).size;
        RulePreset::from_name(&info.rules)
            .map_err(|_| DocError::UnknownRules(info.rules.clone()))?;
        let current = self.game_info()?;
        let komi_changed = !preserve_unchanged || info.komi != current.komi;
        let mut properties = root.properties().to_vec();
        for (name, value, previous) in [
            ("PB", info.black_name, current.black_name),
            ("BR", info.black_rank, current.black_rank),
            ("PW", info.white_name, current.white_name),
            ("WR", info.white_rank, current.white_rank),
        ] {
            if !preserve_unchanged || value != previous {
                replace(&mut properties, name, value);
            }
        }
        if komi_changed {
            replace(&mut properties, "KM", Some(info.komi.to_string()));
        }
        if !preserve_unchanged || info.rules != current.rules_preset {
            replace(&mut properties, "RU", Some(info.rules));
        }
        for (name, value, previous) in [
            ("RE", info.result, current.result),
            ("DT", info.date, current.date),
            ("EV", info.event, current.event),
        ] {
            if !preserve_unchanged || value != previous {
                replace(&mut properties, name, value);
            }
        }
        if komi_changed || metadata::root_metadata(self.root, &properties).komi != info.komi {
            // A rule edit can change how an unchanged source KM/KO is read.
            // Rewrite only when necessary to retain the submitted point value.
            validate_komi(self.root, size, info.komi)?;
            replace(&mut properties, "KM", Some(info.komi.to_string()));
            let unmarked: Vec<_> = properties
                .iter()
                .filter(|property| property.identifier != "LZKOMI")
                .cloned()
                .collect();
            let marker = (metadata::normalize_komi(info.komi, &unmarked) != info.komi)
                .then(|| info.komi.to_string());
            replace(&mut properties, "LZKOMI", marker);
        }
        Ok(properties)
    }
}

fn validate_komi(node: NodeId, size: BoardSize, komi: f64) -> Result<(), DocError> {
    if !komi.is_finite() || komi.abs() >= size.area() as f64 {
        return Err(DocError::InvalidProperty {
            node,
            property: "KM".into(),
            value: komi.to_string(),
        });
    }
    Ok(())
}

fn property(name: &str, value: String) -> Property {
    Property {
        identifier: name.into(),
        values: vec![value],
    }
}

/// Existing positions, including duplicate property positions, stay intact.
/// Newly introduced fields are appended in the command's documented order.
fn replace(properties: &mut Vec<Property>, name: &str, value: Option<String>) {
    let Some(value) = value else {
        properties.retain(|property| property.identifier != name);
        return;
    };
    let mut found = false;
    for existing in properties
        .iter_mut()
        .filter(|property| property.identifier == name)
    {
        existing.values = vec![value.clone()];
        found = true;
    }
    if !found {
        properties.push(property(name, value));
    }
}

fn fixed_handicap(size: BoardSize, handicap: u8) -> Result<Vec<Point>, DocError> {
    if handicap == 0 {
        return Ok(Vec::new());
    }
    if !(2..=9).contains(&handicap)
        || size.width != size.height
        || !matches!(size.width, 9 | 13 | 19)
    {
        return Err(DocError::InvalidHandicap { handicap, size });
    }
    // Board.fixedHandicapPoints defines this topology on 19x19. Java has no
    // local 9/13 placement function; map that topology onto their star points.
    let low = if size.width == 9 { 2 } else { 3 };
    let high = size.width - 1 - low;
    let middle = size.width / 2;
    let coordinates = match handicap {
        2 => vec![(low, high), (high, low)],
        3 => vec![(low, low), (high, low), (low, high)],
        4 => vec![(low, low), (low, high), (high, low), (high, high)],
        5 => vec![
            (low, low),
            (low, high),
            (high, low),
            (high, high),
            (middle, middle),
        ],
        6 => vec![
            (low, low),
            (low, high),
            (high, low),
            (high, high),
            (low, middle),
            (high, middle),
        ],
        7 => vec![
            (low, low),
            (low, high),
            (high, low),
            (high, high),
            (high, middle),
            (low, middle),
            (middle, middle),
        ],
        8 => vec![
            (low, low),
            (low, high),
            (high, low),
            (high, high),
            (middle, low),
            (middle, high),
            (low, middle),
            (high, middle),
        ],
        9 => vec![
            (low, low),
            (low, high),
            (high, low),
            (high, high),
            (middle, low),
            (middle, high),
            (low, middle),
            (high, middle),
            (middle, middle),
        ],
        _ => return Err(DocError::InvalidHandicap { handicap, size }),
    };
    Ok(coordinates
        .into_iter()
        .map(|(x, y)| Point { x, y })
        .collect())
}

#[cfg(test)]
mod tests;
