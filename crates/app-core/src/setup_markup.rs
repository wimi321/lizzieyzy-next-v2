//! Setup and markup commands. SGF editing rules remain in document.

use crate::{AppCore, AppError, PointView, actor};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum MarkupKindView {
    Triangle,
    Square,
    Circle,
    Cross,
    Letter,
    Number,
    Erase,
}

impl From<MarkupKindView> for document::MarkupKind {
    fn from(value: MarkupKindView) -> Self {
        match value {
            MarkupKindView::Triangle => Self::Triangle,
            MarkupKindView::Square => Self::Square,
            MarkupKindView::Circle => Self::Circle,
            MarkupKindView::Cross => Self::Cross,
            MarkupKindView::Letter => Self::Letter,
            MarkupKindView::Number => Self::Number,
            MarkupKindView::Erase => Self::Erase,
        }
    }
}

impl From<document::MarkupKind> for MarkupKindView {
    fn from(value: document::MarkupKind) -> Self {
        match value {
            document::MarkupKind::Triangle => Self::Triangle,
            document::MarkupKind::Square => Self::Square,
            document::MarkupKind::Circle => Self::Circle,
            document::MarkupKind::Cross => Self::Cross,
            document::MarkupKind::Letter => Self::Letter,
            document::MarkupKind::Number => Self::Number,
            document::MarkupKind::Erase => Self::Erase,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MarkupView {
    pub point: PointView,
    pub kind: MarkupKindView,
    pub text: Option<String>,
}

impl From<document::Markup> for MarkupView {
    fn from(value: document::Markup) -> Self {
        Self {
            point: PointView {
                x: value.point.x,
                y: value.point.y,
            },
            kind: value.kind.into(),
            text: value.text,
        }
    }
}

impl From<PointView> for go_core::Point {
    fn from(value: PointView) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

impl AppCore {
    pub async fn set_setup_stone(
        &self,
        point: go_core::Point,
        color: Option<go_core::Color>,
    ) -> Result<u64, AppError> {
        self.apply(document::Command::SetSetupStone { point, color })
            .await
    }

    pub async fn clear_setup(&self) -> Result<u64, AppError> {
        self.apply(document::Command::ClearSetup).await
    }

    pub async fn set_first_player(&self, color: go_core::Color) -> Result<u64, AppError> {
        self.apply(document::Command::SetFirstPlayer(color)).await
    }

    /// Replaces the document, like new_game; the previous tree is not undoable.
    pub async fn convert_to_start_position(&self) -> Result<u64, AppError> {
        self.send(actor::Input::ConvertToStartPosition).await
    }

    pub async fn set_markup(
        &self,
        point: go_core::Point,
        kind: document::MarkupKind,
    ) -> Result<u64, AppError> {
        self.apply(document::Command::SetMarkup { point, kind })
            .await
    }

    pub async fn clear_markup(&self) -> Result<u64, AppError> {
        self.apply(document::Command::ClearMarkup).await
    }

    pub async fn toggle_to_play(&self) -> Result<u64, AppError> {
        self.apply(document::Command::ToggleToPlay).await
    }
}
