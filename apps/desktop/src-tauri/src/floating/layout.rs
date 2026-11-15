//! Explicit custom-layout transactions are the only bulk native placement path.
use super::*;

impl<P: WindowPlatform> Controller<P> {
    pub(super) async fn apply_window_layout(
        &mut self,
        layout: WindowLayout,
    ) -> Result<DisplaySettings, CommandError> {
        layout.validate().map_err(CommandError::DisplaySettings)?;
        let previous = self.core.settings().await?.display;
        let mut actual = Vec::new();
        for kind in [
            WindowKind::Main,
            WindowKind::FloatingMain,
            WindowKind::FloatingSub,
        ] {
            if let Some(value) = self
                .platform
                .placement(kind)
                .map_err(CommandError::WindowUnavailable)?
            {
                actual.push((kind, self.placements.observe(kind, value)));
            }
        }
        let placements = self.placements.clone();
        let reset_bounds = self.reset_bounds.clone();
        let areas = self.areas();
        let mut requested = previous.clone();
        requested.floating_boards = layout.floating_boards;
        if let Some(mut placement) = layout.main_window {
            placement.bounds = self.layout_bounds(WindowKind::Main, placement.bounds, &areas);
            requested.main_window = Some(placement);
        }
        for kind in [FloatingBoardKind::Main, FloatingBoardKind::Sub] {
            let target = window_kind(kind);
            let (given, saved) = match kind {
                FloatingBoardKind::Main => (layout.floating_main, previous.floating_main),
                FloatingBoardKind::Sub => (layout.floating_sub, previous.floating_sub),
            };
            let open = options(&layout.floating_boards, kind).0;
            let bounds = if open {
                Some(
                    self.layout_bounds(
                        target,
                        given
                            .or(saved)
                            .unwrap_or_else(|| window_state::default_bounds(target, &areas)),
                        &areas,
                    ),
                )
            } else if let Some(given) = given {
                Some(self.layout_bounds(target, given, &areas))
            } else {
                // Keep the last native location when this transaction closes a
                // board, without committing anything until every effect succeeds.
                actual
                    .iter()
                    .find(|(kind, _)| *kind == target)
                    .map(|(_, value)| value.bounds)
                    .or(saved)
            };
            match kind {
                FloatingBoardKind::Main => requested.floating_main = bounds,
                FloatingBoardKind::Sub => requested.floating_sub = bounds,
            }
        }
        let prepared = WindowLayout::from(&requested);
        prepared.validate().map_err(CommandError::DisplaySettings)?;
        let effect = self.place_layout(&requested, layout.main_window.is_some());
        if let Err(error) = effect {
            self.rollback(&previous, &actual);
            self.placements = placements;
            self.reset_bounds = reset_bounds;
            return Err(error);
        }
        match self.core.save_window_layout(prepared).await {
            Ok(display) => Ok(display),
            Err(error) => {
                self.rollback(&previous, &actual);
                self.placements = placements;
                self.reset_bounds = reset_bounds;
                Err(error.into())
            }
        }
    }

    fn layout_bounds(
        &self,
        kind: WindowKind,
        bounds: WindowBounds,
        areas: &[WindowBounds],
    ) -> WindowBounds {
        if areas.is_empty() {
            window_state::default_bounds(kind, areas)
        } else {
            fit_window_bounds(bounds, areas, self.platform.minimum(kind))
        }
    }

    fn place_layout(
        &mut self,
        display: &DisplaySettings,
        place_main: bool,
    ) -> Result<(), CommandError> {
        if place_main && let Some(placement) = display.main_window {
            self.place_layout_window(WindowKind::Main, placement)?;
        }
        self.reconcile(display)?;
        for (kind, open, bounds) in [
            (
                WindowKind::FloatingMain,
                display.floating_boards.main,
                display.floating_main,
            ),
            (
                WindowKind::FloatingSub,
                display.floating_boards.sub,
                display.floating_sub,
            ),
        ] {
            // Apply even when the value equals the saved setting: the native
            // window might have moved since the last debounced save.
            if open && let Some(bounds) = bounds {
                self.place_layout_window(
                    kind,
                    WindowPlacement {
                        bounds,
                        maximized: false,
                    },
                )?;
            }
        }
        Ok(())
    }

    fn place_layout_window(
        &mut self,
        kind: WindowKind,
        placement: WindowPlacement,
    ) -> Result<(), CommandError> {
        self.placements.seed(kind, placement.bounds);
        self.reset_bounds.remove(window_state::label(kind));
        self.platform
            .place(kind, placement)
            .map_err(CommandError::WindowUnavailable)
    }
}
