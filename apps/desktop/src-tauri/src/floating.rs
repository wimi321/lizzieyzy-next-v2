//! Serialized window effects; AppCore remains the owner of persisted settings.
use app_core::{
    AppCore, DisplaySettings, FloatingBoardKind, FloatingBoardsView, WindowBounds, WindowKind,
    WindowLayout, WindowPlacement, fit_window_bounds,
};
use tokio::sync::{mpsc, oneshot};

use crate::{
    error::CommandError,
    window_state::{self, Placements},
};

mod layout;
mod native;
pub(crate) use native::on_window_event;
#[cfg(test)]
mod tests;

pub(crate) trait WindowPlatform: Send + 'static {
    fn work_areas(&self) -> Result<Vec<WindowBounds>, String>;
    /// Total horizontal and vertical window decorations in logical pixels.
    fn frame(&self, _kind: WindowKind) -> (u32, u32) {
        (0, 0)
    }
    fn minimum(&self, kind: WindowKind) -> (u32, u32) {
        let base = window_state::minimum(kind);
        let frame = self.frame(kind);
        (
            base.0.saturating_add(frame.0),
            base.1.saturating_add(frame.1),
        )
    }
    fn exists(&self, kind: WindowKind) -> bool;
    fn create(
        &mut self,
        kind: FloatingBoardKind,
        bounds: WindowBounds,
        on_top: bool,
        locked: bool,
    ) -> Result<(), String>;
    fn destroy(&mut self, kind: WindowKind) -> Result<(), String>;
    fn placement(&self, kind: WindowKind) -> Result<Option<WindowPlacement>, String>;
    fn place(&mut self, kind: WindowKind, placement: WindowPlacement) -> Result<(), String>;
    fn options(
        &mut self,
        kind: FloatingBoardKind,
        on_top: bool,
        locked: bool,
    ) -> Result<(), String>;
}

pub(crate) enum Action {
    Open(FloatingBoardKind, bool),
    Options(FloatingBoardKind, bool, bool),
    Layout(bool, bool, bool),
    Fit(u32, u32),
    Reset,
    ApplyLayout(WindowLayout),
    Flush,
}

enum Request {
    Action(
        Action,
        oneshot::Sender<Result<DisplaySettings, CommandError>>,
    ),
    Changed(WindowKind),
}

#[derive(Clone)]
pub(crate) struct FloatingWindows {
    requests: mpsc::Sender<Request>,
}

impl FloatingWindows {
    pub(crate) fn for_app<R: tauri::Runtime>(app: tauri::AppHandle<R>, core: AppCore) -> Self {
        Self::spawn(native::NativeWindows(app), core)
    }

    pub(crate) fn spawn(platform: impl WindowPlatform, core: AppCore) -> Self {
        let (requests, receiver) = mpsc::channel(64);
        tauri::async_runtime::spawn(
            Controller {
                core,
                platform,
                placements: Placements::default(),
                reset_bounds: Default::default(),
            }
            .run(receiver),
        );
        Self { requests }
    }

    pub(crate) async fn apply(&self, action: Action) -> Result<DisplaySettings, CommandError> {
        let (reply, response) = oneshot::channel();
        self.requests
            .send(Request::Action(action, reply))
            .await
            .map_err(|_| CommandError::StreamClosed)?;
        response.await.map_err(|_| CommandError::StreamClosed)?
    }

    pub(crate) fn changed(&self, kind: WindowKind) {
        // A full queue already contains movement notifications. The controller
        // samples the latest native bounds at the deadline, never stale payloads.
        let _ = self.requests.try_send(Request::Changed(kind));
    }
}

struct Controller<P> {
    core: AppCore,
    platform: P,
    placements: Placements,
    // Native notifications caused by reset must not immediately repopulate the
    // cleared saved positions. A subsequent user move lifts this suppression.
    reset_bounds: std::collections::BTreeMap<&'static str, WindowPlacement>,
}

pub(crate) fn window_kind(kind: FloatingBoardKind) -> WindowKind {
    match kind {
        FloatingBoardKind::Main => WindowKind::FloatingMain,
        FloatingBoardKind::Sub => WindowKind::FloatingSub,
    }
}

fn options(view: &FloatingBoardsView, kind: FloatingBoardKind) -> (bool, bool, bool) {
    match kind {
        FloatingBoardKind::Main => (view.main, view.main_on_top, view.main_locked),
        FloatingBoardKind::Sub => (view.sub, view.sub_on_top, view.sub_locked),
    }
}

impl<P: WindowPlatform> Controller<P> {
    fn areas(&self) -> Vec<WindowBounds> {
        match self.platform.work_areas() {
            Ok(areas) => areas,
            Err(error) => {
                eprintln!("window monitors: {error}");
                Vec::new()
            }
        }
    }

    async fn initialize(&mut self) {
        let Ok(settings) = self.core.settings().await else {
            return;
        };
        let areas = self.areas();
        if let Some(mut placement) = settings.display.main_window {
            if !areas.is_empty() {
                placement.bounds = fit_window_bounds(
                    placement.bounds,
                    &areas,
                    self.platform.minimum(WindowKind::Main),
                );
                self.placements.seed(WindowKind::Main, placement.bounds);
                if let Err(error) = self.platform.place(WindowKind::Main, placement) {
                    eprintln!("restore main window: {error}");
                }
            }
        } else {
            match self.platform.placement(WindowKind::Main) {
                Ok(Some(value)) => {
                    // A layout can be saved before any move or resize event arrives.
                    self.placements.seed(WindowKind::Main, value.bounds);
                    if let Err(error) = self
                        .core
                        .save_window_placement(WindowKind::Main, value)
                        .await
                    {
                        eprintln!("save initial main window placement: {error}");
                    }
                }
                Ok(None) => eprintln!("initial main window placement is unavailable"),
                Err(error) => eprintln!("read initial main window placement: {error}"),
            }
        }
        // A missing screen/window is optional: continue restoring the other
        // window and keep the main application running.
        for kind in [FloatingBoardKind::Main, FloatingBoardKind::Sub] {
            if let Err(error) = self.reconcile_one(&settings.display, kind) {
                eprintln!("restore floating window: {error}");
                if let Err(error) = self.core.set_floating_board_open(kind, false).await {
                    eprintln!("floating state: {error}");
                }
            }
        }
    }

    async fn run(mut self, mut requests: mpsc::Receiver<Request>) {
        self.initialize().await;
        loop {
            let deadline = self.placements.deadline();
            tokio::select! {
                request = requests.recv() => match request {
                    Some(Request::Changed(kind)) => {
                        // Track normal geometry immediately; only disk writes
                        // are debounced. Maximizing must not replace it with
                        // the screen-sized maximized rectangle.
                        if let Ok(Some(value)) = self.platform.placement(kind) {
                            self.placements.observe(kind, value);
                        }
                        self.placements.changed(kind);
                    },
                    Some(Request::Action(action, reply)) => { let _ = reply.send(self.action(action).await); }
                    None => { self.flush().await; break; }
                },
                _ = async { if let Some(deadline) = deadline { tokio::time::sleep_until(deadline).await; } else { std::future::pending::<()>().await; } } => {
                    for kind in self.placements.due() { self.save(kind).await; }
                }
            }
        }
    }

    fn reconcile_one(
        &mut self,
        display: &DisplaySettings,
        kind: FloatingBoardKind,
    ) -> Result<(), CommandError> {
        let (open, on_top, locked) = options(&display.floating_boards, kind);
        let target = window_kind(kind);
        let exists = self.platform.exists(target);
        let result = if open && !exists {
            let areas = self.areas();
            let saved = match kind {
                FloatingBoardKind::Main => display.floating_main,
                FloatingBoardKind::Sub => display.floating_sub,
            };
            let bounds = if areas.is_empty() {
                window_state::default_bounds(target, &areas)
            } else {
                fit_window_bounds(
                    saved.unwrap_or_else(|| window_state::default_bounds(target, &areas)),
                    &areas,
                    self.platform.minimum(target),
                )
            };
            self.placements.seed(target, bounds);
            self.platform.create(kind, bounds, on_top, locked)
        } else if !open && exists {
            self.platform.destroy(target)
        } else if exists {
            self.platform.options(kind, on_top, locked)
        } else {
            Ok(())
        };
        result.map_err(CommandError::WindowUnavailable)
    }

    fn reconcile(&mut self, display: &DisplaySettings) -> Result<(), CommandError> {
        for kind in [FloatingBoardKind::Main, FloatingBoardKind::Sub] {
            self.reconcile_one(display, kind)?;
        }
        Ok(())
    }

    async fn save(&mut self, kind: WindowKind) {
        let value = match self.platform.placement(kind) {
            Ok(Some(value)) => self.placements.observe(kind, value),
            Ok(None) => return,
            Err(error) => {
                eprintln!("read window placement: {error}");
                return;
            }
        };
        if self.reset_bounds.get(window_state::label(kind)) == Some(&value) {
            return;
        }
        self.reset_bounds.remove(window_state::label(kind));
        if let Err(error) = self.core.save_window_placement(kind, value).await {
            eprintln!("save window placement: {error}");
        }
    }

    async fn flush(&mut self) {
        for kind in [
            WindowKind::Main,
            WindowKind::FloatingMain,
            WindowKind::FloatingSub,
        ] {
            self.save(kind).await;
        }
        self.placements.clear();
    }

    async fn action(&mut self, action: Action) -> Result<DisplaySettings, CommandError> {
        if matches!(action, Action::Flush) {
            self.flush().await;
            return Ok(self.core.settings().await?.display);
        }
        if let Action::Fit(width, height) = action {
            return self.fit(width, height).await;
        }
        if matches!(action, Action::Reset) {
            return self.reset().await;
        }
        if let Action::ApplyLayout(layout) = action {
            return self.apply_window_layout(layout).await;
        }
        let previous = self.core.settings().await?.display;
        let actual: Vec<_> = [
            WindowKind::Main,
            WindowKind::FloatingMain,
            WindowKind::FloatingSub,
        ]
        .into_iter()
        .filter_map(|kind| {
            self.platform
                .placement(kind)
                .ok()
                .flatten()
                .map(|value| (kind, value))
        })
        .collect();
        let mut requested = previous.clone();
        match &action {
            Action::Open(kind, open) => match kind {
                FloatingBoardKind::Main => requested.floating_boards.main = *open,
                FloatingBoardKind::Sub => requested.floating_boards.sub = *open,
            },
            Action::Options(kind, top, locked) => match kind {
                FloatingBoardKind::Main => {
                    requested.floating_boards.main_on_top = *top;
                    requested.floating_boards.main_locked = *locked;
                }
                FloatingBoardKind::Sub => {
                    requested.floating_boards.sub_on_top = *top;
                    requested.floating_boards.sub_locked = *locked;
                }
            },
            Action::Layout(main, keep, sub) => {
                requested.floating_boards.main = *main;
                requested.floating_boards.keep_main_board = *keep;
                requested.floating_boards.sub = *sub;
            }
            _ => {}
        }
        requested
            .validate()
            .map_err(CommandError::DisplaySettings)?;
        // Capture before destruction, while native bounds still exist.
        for kind in [FloatingBoardKind::Main, FloatingBoardKind::Sub] {
            if !options(&requested.floating_boards, kind).0
                && self.platform.exists(window_kind(kind))
            {
                self.save(window_kind(kind)).await;
            }
        }
        let effect = self.reconcile(&requested);
        if let Err(error) = effect {
            self.rollback(&previous, &actual);
            return Err(error);
        }
        let saved = match action {
            Action::Open(kind, open) => self
                .core
                .set_floating_board_open(kind, open)
                .await
                .map(|_| ()),
            Action::Options(kind, top, locked) => self
                .core
                .set_floating_board_options(kind, top, locked)
                .await
                .map(|_| ()),
            Action::Layout(main, keep, sub) => self
                .core
                .set_floating_layout(main, keep, sub)
                .await
                .map(|_| ()),
            _ => Ok(()),
        };
        if let Err(error) = saved {
            self.rollback(&previous, &actual);
            return Err(error.into());
        }
        Ok(self.core.settings().await?.display)
    }

    fn rollback(&mut self, previous: &DisplaySettings, actual: &[(WindowKind, WindowPlacement)]) {
        if let Err(error) = self.reconcile(previous) {
            eprintln!("restore windows after failure: {error}");
        }
        for (kind, placement) in actual {
            if self.platform.exists(*kind)
                && let Err(error) = self.platform.place(*kind, *placement)
            {
                eprintln!("restore placement after failure: {error}");
            }
        }
    }

    async fn fit(&mut self, width: u32, height: u32) -> Result<DisplaySettings, CommandError> {
        if width == 0 || height == 0 || width > 100_000 || height > 100_000 {
            return Err(CommandError::WindowUnavailable(
                "window size must be in 1..=100000".into(),
            ));
        }
        if let Some(mut value) = self
            .platform
            .placement(WindowKind::Main)
            .map_err(CommandError::WindowUnavailable)?
            && !value.maximized
        {
            // The caller measures client size; fitting and persistence use outer bounds.
            let frame = self.platform.frame(WindowKind::Main);
            value.bounds.width = width.saturating_add(frame.0);
            value.bounds.height = height.saturating_add(frame.1);
            value.bounds = fit_window_bounds(
                value.bounds,
                &self.areas(),
                self.platform.minimum(WindowKind::Main),
            );
            self.platform
                .place(WindowKind::Main, value)
                .map_err(CommandError::WindowUnavailable)?;
            self.placements.seed(WindowKind::Main, value.bounds);
            self.save(WindowKind::Main).await;
        }
        Ok(self.core.settings().await?.display)
    }

    async fn reset(&mut self) -> Result<DisplaySettings, CommandError> {
        let display = self.core.reset_window_positions().await?;
        self.placements.clear();
        let areas = self.areas();
        let mut failure = None;
        for kind in [
            WindowKind::Main,
            WindowKind::FloatingMain,
            WindowKind::FloatingSub,
        ] {
            if self.platform.exists(kind) {
                let value = WindowPlacement {
                    bounds: window_state::default_bounds(kind, &areas),
                    maximized: false,
                };
                self.placements.seed(kind, value.bounds);
                self.reset_bounds.insert(window_state::label(kind), value);
                if let Err(error) = self.platform.place(kind, value) {
                    failure = Some(error);
                }
            }
        }
        if let Some(error) = failure {
            return Err(CommandError::WindowUnavailable(error));
        }
        Ok(display)
    }
}
