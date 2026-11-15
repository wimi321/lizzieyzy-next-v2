use super::Actor;
use crate::{
    FloatingBoardKind, WindowKind,
    window_api::{Request, Update},
};

impl Actor {
    pub(super) async fn window_request(&mut self, request: Request) {
        let mut display = self.settings.display.clone();
        match request.update {
            Update::Snapshot(layout) => layout.apply_to(&mut display),
            Update::Layout(main, keep_main_board, sub) => {
                display.floating_boards.main = main;
                display.floating_boards.keep_main_board = keep_main_board;
                display.floating_boards.sub = sub;
            }
            Update::Open(kind, open) => match kind {
                FloatingBoardKind::Main => display.floating_boards.main = open,
                FloatingBoardKind::Sub => display.floating_boards.sub = open,
            },
            Update::Options(kind, on_top, locked) => match kind {
                FloatingBoardKind::Main => {
                    display.floating_boards.main_on_top = on_top;
                    display.floating_boards.main_locked = locked;
                }
                FloatingBoardKind::Sub => {
                    display.floating_boards.sub_on_top = on_top;
                    display.floating_boards.sub_locked = locked;
                }
            },
            Update::Placement(kind, placement) => match kind {
                WindowKind::Main => display.main_window = Some(placement),
                WindowKind::FloatingMain => display.floating_main = Some(placement.bounds),
                WindowKind::FloatingSub => display.floating_sub = Some(placement.bounds),
            },
            Update::Reset => {
                display.main_window = None;
                display.floating_main = None;
                display.floating_sub = None;
                display.board_position = 4;
            }
        }
        let result = if display == self.settings.display {
            Ok(display)
        } else {
            self.persist_display_settings(display).await
        };
        let _ = request.reply.send(result);
    }
}
