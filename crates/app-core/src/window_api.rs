//! Serialized partial updates keep geometry writes from overwriting UI settings.
use crate::{
    AppCore, AppError, DisplaySettings, FloatingBoardKind, FloatingBoardsView, WindowKind,
    WindowLayout, WindowPlacement, actor,
};
use tokio::sync::oneshot;

pub(crate) enum Update {
    Snapshot(WindowLayout),
    Layout(bool, bool, bool),
    Open(FloatingBoardKind, bool),
    Options(FloatingBoardKind, bool, bool),
    Placement(WindowKind, WindowPlacement),
    Reset,
}

pub(crate) struct Request {
    pub update: Update,
    pub reply: oneshot::Sender<Result<DisplaySettings, AppError>>,
}

impl AppCore {
    async fn update_window_settings(&self, update: Update) -> Result<DisplaySettings, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::Windows(Request { update, reply }))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    pub async fn set_floating_layout(
        &self,
        main: bool,
        keep_main_board: bool,
        sub: bool,
    ) -> Result<FloatingBoardsView, AppError> {
        Ok(self
            .update_window_settings(Update::Layout(main, keep_main_board, sub))
            .await?
            .floating_boards)
    }

    pub async fn set_floating_board_open(
        &self,
        kind: FloatingBoardKind,
        open: bool,
    ) -> Result<FloatingBoardsView, AppError> {
        Ok(self
            .update_window_settings(Update::Open(kind, open))
            .await?
            .floating_boards)
    }

    pub async fn set_floating_board_options(
        &self,
        kind: FloatingBoardKind,
        on_top: bool,
        locked: bool,
    ) -> Result<FloatingBoardsView, AppError> {
        Ok(self
            .update_window_settings(Update::Options(kind, on_top, locked))
            .await?
            .floating_boards)
    }

    pub async fn save_window_placement(
        &self,
        kind: WindowKind,
        placement: WindowPlacement,
    ) -> Result<(), AppError> {
        self.update_window_settings(Update::Placement(kind, placement))
            .await
            .map(|_| ())
    }

    /// Atomically replace exactly the four resolved window-owned fields.
    /// The shell resolves omitted positions before committing this snapshot.
    pub async fn save_window_layout(
        &self,
        layout: WindowLayout,
    ) -> Result<DisplaySettings, AppError> {
        self.update_window_settings(Update::Snapshot(layout)).await
    }

    pub async fn reset_window_positions(&self) -> Result<DisplaySettings, AppError> {
        self.update_window_settings(Update::Reset).await
    }
}
