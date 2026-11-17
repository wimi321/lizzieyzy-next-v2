use super::Actor;
use crate::{
    AppError, AppEvent, CaptureError, CaptureImage, CaptureOutcome, CaptureResult, CaptureSession,
    CaptureSettings, EngineRules, capture::Request,
};
use document::GameDocument;
use go_core::{Color, Point};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct State {
    sequence: u32,
    session: Option<(CaptureSession, Arc<CaptureImage>)>,
}

impl State {
    fn image(&self, id: u32) -> Result<Arc<CaptureImage>, CaptureError> {
        let (session, image) = self.session.as_ref().ok_or(CaptureError::NoSession)?;
        if session.id != id {
            return Err(CaptureError::StaleSession);
        }
        Ok(image.clone())
    }

    fn begin(&mut self, image: CaptureImage) -> Result<CaptureSession, CaptureError> {
        image.validate()?;
        let id = self
            .sequence
            .checked_add(1)
            .ok_or(CaptureError::SessionLimit)?;
        let session = CaptureSession {
            id,
            width: image.width,
            height: image.height,
            scale_factor: image.scale_factor,
        };
        self.sequence = id;
        self.session = Some((session, Arc::new(image)));
        Ok(session)
    }
}

impl Actor {
    pub(super) fn capture_command(&mut self, request: Request) {
        match request {
            Request::Begin(image, reply) => {
                let result = if self.updates.install_pending {
                    Err(crate::updates::UpdateError::Busy.into())
                } else {
                    self.capture.begin(image).map_err(Into::into)
                };
                let _ = reply.send(result);
            }
            Request::Image(id, reply) => {
                let _ = reply.send(self.capture.image(id).map_err(Into::into));
            }
            Request::Prepare(id, reply) => {
                let result = self
                    .capture
                    .image(id)
                    .map(|image| (image, self.settings.capture));
                let _ = reply.send(result.map_err(Into::into));
            }
            Request::Validate(id, reply) => {
                let _ = reply.send(self.capture.image(id).map(|_| ()).map_err(Into::into));
            }
            Request::Cancel(id, reply) => {
                let _ = reply.send(self.end_capture(id, CaptureOutcome::Cancelled));
            }
            Request::Fail(id, reason, reply) => {
                let _ = reply.send(self.end_capture(id, CaptureOutcome::Failed { reason }));
            }
            Request::ReportFailure(reason, reply) => {
                self.output.send(AppEvent::CaptureEnded {
                    outcome: CaptureOutcome::Failed { reason },
                });
                let _ = reply.send(Ok(()));
            }
        }
    }

    pub(super) async fn set_capture_settings(
        &mut self,
        capture: CaptureSettings,
    ) -> Result<(), AppError> {
        capture.validate()?;
        let mut settings = self.settings.clone();
        settings.capture = capture;
        settings.save(&self.directory).await?;
        self.settings = settings;
        Ok(())
    }

    pub(super) async fn finish_capture(
        &mut self,
        id: u32,
        result: CaptureResult,
    ) -> Result<(), AppError> {
        self.capture.image(id)?;
        self.ensure_no_move_trial()?;
        let stones = board_capture::place(
            &result.cells.into(),
            result.corner.into(),
            result.board_size,
        )
        .map_err(CaptureError::from)?;
        let rules = match &self.settings.engine_settings.rules {
            EngineRules::FollowSgf | EngineRules::Chinese => "chinese",
            EngineRules::Japanese => "japanese",
            EngineRules::Korean => "korean",
            EngineRules::Aga => "aga",
            EngineRules::TrompTaylor => "tromp-taylor",
            EngineRules::NewZealand => "new-zealand",
            _ => return Err(CaptureError::UnsupportedRules.into()),
        };
        // Reuse new_game for metadata/komi validation, then materialize a setup
        // root just like generate_tsumego. No SGF text or played move chain.
        let document = GameDocument::new_game(document::NewGameOptions {
            width: result.board_size,
            height: result.board_size,
            komi: self.settings.engine_settings.default_komi,
            rules: rules.into(),
            ..Default::default()
        })?;
        let size = go_core::BoardSize::new(result.board_size, result.board_size)
            .map_err(|_| CaptureError::InvalidBoardSize)?;
        let mut tree = document.to_sgf();
        let root = tree
            .sequence
            .first_mut()
            .ok_or(CaptureError::InvalidCells)?;
        root.properties
            .retain(|property| property.identifier != "PL");
        root.properties.push(sgf::Property {
            identifier: "PL".into(),
            values: vec!["B".into()],
        });
        for (color, identifier) in [(Color::Black, "AB"), (Color::White, "AW")] {
            let values = stones
                .iter()
                .enumerate()
                .filter(|(_, stone)| **stone == Some(color))
                .map(|(index, _)| {
                    Point {
                        x: (index % usize::from(result.board_size)) as u8,
                        y: (index / usize::from(result.board_size)) as u8,
                    }
                    .to_sgf(size)
                    .map_err(analysis::AnalysisError::from)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !values.is_empty() {
                root.properties.push(sgf::Property {
                    identifier: identifier.into(),
                    values,
                });
            }
        }
        let document = GameDocument::from_sgf(&tree)?;
        document.position_at(document.root())?;
        self.stop_board_sync().await?;
        self.open_auto_curve_document(document, None, false).await?;
        self.end_capture(id, CaptureOutcome::Finished)
    }

    fn end_capture(&mut self, id: u32, outcome: CaptureOutcome) -> Result<(), AppError> {
        self.capture.image(id)?;
        self.capture.session = None;
        self.output.send(AppEvent::CaptureEnded { outcome });
        Ok(())
    }
}
