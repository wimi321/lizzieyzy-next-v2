use super::Actor;
use crate::{AppError, ColorView, TsumegoError, TsumegoOptions, TsumegoStatus};
use document::GameDocument;
use go_core::{Color, Point};
use sgf::{GameTree, Node, Property};

pub(super) struct Session {
    original: GameDocument,
    attacker: ColorView,
    ko_warnings: Vec<ColorView>,
}

impl Actor {
    pub(super) fn tsumego_status(&self) -> TsumegoStatus {
        TsumegoStatus {
            revision: self.revision,
            active: self.tsumego.is_some(),
            source_node: self.tsumego.as_ref().map(|s| s.original.current().0),
            options: self.settings.tsumego,
            attacker: self.tsumego.as_ref().map(|s| s.attacker),
            ko_warnings: self
                .tsumego
                .as_ref()
                .map(|s| s.ko_warnings.clone())
                .unwrap_or_default(),
        }
    }

    pub(super) async fn generate_tsumego(
        &mut self,
        options: TsumegoOptions,
    ) -> Result<(), AppError> {
        options.validate()?;
        let original = self
            .tsumego
            .as_ref()
            .map_or(&self.document, |session| &session.original);
        let position = original.position_at(original.current())?;
        let size = position.board.size();
        let generated = go_core::tsumego::generate(
            size,
            position.board.stones(),
            position.board.to_play(),
            options.into(),
        )
        .map_err(TsumegoError::from)?;
        let komi = generated.komi.ok_or(TsumegoError::NoSpace)?;
        let mut properties = vec![
            property("SZ", format!("{}:{}", size.width, size.height)),
            property("RU", position.rules.as_katago_str().to_owned()),
            property("KM", komi.to_string()),
            property("PL", sgf_color(generated.to_play).to_owned()),
        ];
        for (color, identifier) in [(Color::Black, "AB"), (Color::White, "AW")] {
            let values = generated
                .stones
                .iter()
                .enumerate()
                .filter(|(_, stone)| **stone == Some(color))
                .map(|(index, _)| {
                    Point {
                        x: (index % usize::from(size.width)) as u8,
                        y: (index / usize::from(size.width)) as u8,
                    }
                    .to_sgf(size)
                    .map_err(analysis::AnalysisError::from)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !values.is_empty() {
                properties.push(Property {
                    identifier: identifier.to_owned(),
                    values,
                });
            }
        }
        let document = GameDocument::from_sgf(&GameTree {
            sequence: vec![Node { properties }],
            children: Vec::new(),
        })?;
        // A Java-generated arrangement is not allowed to bypass the domain's
        // legal setup validation. Reject atomically rather than analyze fiction.
        document.position_at(document.root())?;
        self.next_revision()?;
        let mut settings = self.settings.clone();
        settings.tsumego = options;
        settings.save(&self.directory).await?;

        let previous = std::mem::replace(&mut self.document, document);
        let original = self
            .tsumego
            .take()
            .map_or(previous, |session| session.original);
        self.tsumego = Some(Session {
            original,
            attacker: generated.attacker.into(),
            ko_warnings: generated.ko_warnings.into_iter().map(Into::into).collect(),
        });
        self.settings = settings;
        self.document_replaced().await
    }

    pub(super) async fn exit_tsumego(&mut self) -> Result<(), AppError> {
        if self.tsumego.is_none() {
            return Ok(());
        }
        self.next_revision()?;
        if let Some(session) = self.tsumego.take() {
            self.document = session.original;
        }
        self.document_replaced().await
    }
}

fn property(identifier: &str, value: String) -> Property {
    Property {
        identifier: identifier.to_owned(),
        values: vec![value],
    }
}

fn sgf_color(color: Color) -> &'static str {
    match color {
        Color::Black => "B",
        Color::White => "W",
    }
}
