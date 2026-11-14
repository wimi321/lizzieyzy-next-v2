//! Queue state and advancement stay in the main document actor.
use super::{Actor, Command, Input};
use crate::{
    AppError, AppEvent, BatchAnalysisStatus, BatchError, BatchItem, BatchItemState, BatchMove,
    BatchOptions, BatchPhase, WholeGameSource, WholeGameStatus, batch::Request,
};
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct State {
    pub status: BatchAnalysisStatus,
    options: Option<BatchOptions>,
    next_id: u32,
}

impl Actor {
    pub(super) fn batch_active(&self) -> bool {
        matches!(
            self.batch.status.phase,
            BatchPhase::Running | BatchPhase::Paused
        )
    }

    pub(super) fn ensure_no_batch(&self) -> Result<(), AppError> {
        if self.batch_active() {
            Err(AppError::BatchAnalysisRunning)
        } else {
            Ok(())
        }
    }

    pub(super) fn batch_guard(&self, command: Command) -> Option<Command> {
        if !self.batch_active() {
            return Some(command);
        }
        macro_rules! reject {
            ($reply:expr) => {{
                let _ = $reply.send(Err(AppError::BatchAnalysisRunning));
                None
            }};
        }
        match command {
            Command::Apply(input, reply) if blocked_input(&input) => {
                match &input {
                    Input::OpenPath(path) => self.output.send(AppEvent::OpenFailed {
                        path: path.to_string_lossy().into_owned(),
                        reason: AppError::BatchAnalysisRunning.to_string(),
                    }),
                    Input::OpenDroppedPaths(paths) => {
                        if let Some(path) =
                            paths.iter().find(|p| super::game_files::is_game_file(p))
                        {
                            self.output.send(AppEvent::OpenFailed {
                                path: path.to_string_lossy().into_owned(),
                                reason: AppError::BatchAnalysisRunning.to_string(),
                            });
                        }
                    }
                    _ => {}
                }
                reject!(reply)
            }
            Command::Review(_, reply) => reject!(reply),
            Command::BoardSync(crate::board_sync::Request::Start(_, _), reply) => reject!(reply),
            Command::Benchmark(
                crate::benchmark::Request::Start | crate::benchmark::Request::Reset,
                reply,
            ) => reject!(reply),
            other => Some(other),
        }
    }

    pub(super) async fn batch_command(
        &mut self,
        request: Request,
    ) -> Result<BatchAnalysisStatus, AppError> {
        match request {
            Request::Start(options) => {
                self.ensure_no_batch()?;
                self.ensure_no_benchmark()?;
                self.ensure_no_move_trial()?;
                options.validate()?;
                if options.files.is_empty() && !self.batch.status.items.iter().any(waiting) {
                    return Err(BatchError::EmptyQueue.into());
                }
                self.batch_capacity(options.files.len())?;
                self.stop_board_sync().await?;
                self.add_batch_files(options.files.clone())?;
                self.cancel_auto_curve_intent();
                self.cancel_review().await;
                self.batch.options = Some(options);
                self.batch.status.phase = BatchPhase::Running;
                self.batch.status.reason = None;
            }
            Request::Pause => {
                if self.batch.status.phase == BatchPhase::Running {
                    if self.batch.status.current_index.is_some() {
                        self.pause_review().await?;
                    }
                    self.batch.status.phase = BatchPhase::Paused;
                }
            }
            Request::Resume => {
                if self.batch.status.phase != BatchPhase::Paused {
                    return Err(BatchError::NotPaused.into());
                }
                if self.batch.status.current_index.is_some() {
                    self.resume_review().await?;
                }
                self.batch.status.phase = BatchPhase::Running;
            }
            Request::Stop => self.stop_batch(None).await,
            Request::Add(files) => self.add_batch_files(files)?,
            Request::Remove(id) => {
                let index = self.waiting_batch_index(id)?;
                self.batch.status.items.remove(index);
                self.reindex_batch();
            }
            Request::Move(id, direction) => {
                let index = self.waiting_batch_index(id)?;
                let indices: Vec<_> = self
                    .batch
                    .status
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| waiting(item))
                    .map(|(i, _)| i)
                    .collect();
                let target = match direction {
                    BatchMove::Up => indices.iter().copied().rev().find(|i| *i < index),
                    BatchMove::Down => indices.iter().copied().find(|i| *i > index),
                    BatchMove::First => indices.first().copied(),
                };
                if let Some(target) = target {
                    let item = self.batch.status.items.remove(index);
                    self.batch.status.items.insert(target, item);
                    self.reindex_batch();
                }
            }
            Request::Clear => {
                self.batch
                    .status
                    .items
                    .retain(|item| matches!(item.state, BatchItemState::Running { .. }));
                self.reindex_batch();
                if !self.batch_active() {
                    self.batch.status.phase = BatchPhase::Idle;
                    self.batch.status.reason = None;
                }
            }
        }
        self.copy_batch_progress();
        self.publish_batch();
        Ok(self.batch.status.clone())
    }

    fn batch_capacity(&self, count: usize) -> Result<u32, AppError> {
        let count = u32::try_from(count).map_err(|_| BatchError::IdLimit)?;
        self.batch
            .next_id
            .checked_add(count)
            .ok_or_else(|| BatchError::IdLimit.into())
    }

    fn add_batch_files(&mut self, files: Vec<PathBuf>) -> Result<(), AppError> {
        let next_id = self.batch_capacity(files.len())?;
        for (offset, path) in files.into_iter().enumerate() {
            self.batch.status.items.push(BatchItem {
                id: self.batch.next_id + offset as u32,
                path,
                state: BatchItemState::Waiting,
            });
        }
        self.batch.next_id = next_id;
        Ok(())
    }

    fn waiting_batch_index(&self, id: u32) -> Result<usize, AppError> {
        self.batch
            .status
            .items
            .iter()
            .position(|item| item.id == id && waiting(item))
            .ok_or_else(|| BatchError::NotWaiting.into())
    }

    fn reindex_batch(&mut self) {
        self.batch.status.current_index = self
            .batch
            .status
            .items
            .iter()
            .position(|item| matches!(item.state, BatchItemState::Running { .. }));
    }

    fn publish_batch(&self) {
        self.output
            .send(AppEvent::BatchAnalysisProgress(self.batch.status.clone()));
    }

    pub(super) async fn stop_batch(&mut self, reason: Option<String>) {
        if let Some(index) = self.batch.status.current_index.take() {
            self.batch.status.items[index].state = BatchItemState::Waiting;
        }
        if self.batch_active() {
            self.cancel_review().await;
        }
        self.batch.status.phase = BatchPhase::Stopped;
        self.batch.status.reason = reason;
        self.publish_batch();
    }

    pub(super) fn batch_needs_advance(&self) -> bool {
        self.batch.status.phase == BatchPhase::Running && self.batch.status.current_index.is_none()
    }

    fn batch_engine_error(&self) -> Option<String> {
        let Some(running) = &self.running else {
            return Some("analysis engine is unavailable".into());
        };
        match &*running.engine_status.borrow() {
            engine_session::EngineStatus::Failed { reason, .. } => Some(reason.clone()),
            engine_session::EngineStatus::Stopped => Some("analysis engine stopped".into()),
            _ => None,
        }
    }

    /// One item per actor turn, so commands can interleave even with a queue of
    /// unreadable files or fully cached games. No separate scheduler is spawned.
    pub(super) async fn advance_batch(&mut self) {
        if !self.batch_needs_advance() {
            return;
        }
        if let Some(reason) = self.batch_engine_error() {
            self.stop_batch(Some(reason)).await;
            return;
        }
        let Some(index) = self.batch.status.items.iter().position(waiting) else {
            self.batch.status.phase = BatchPhase::Completed;
            self.publish_batch();
            return;
        };
        self.batch.status.current_index = Some(index);
        self.batch.status.items[index].state = BatchItemState::Running { progress: None };
        self.publish_batch();
        let result = self.start_batch_item(index).await;
        if let Err(error) = result {
            if let Some(reason) = self.batch_engine_error() {
                self.stop_batch(Some(reason)).await;
            } else {
                self.finish_batch_item(BatchItemState::Failed {
                    reason: error.to_string(),
                });
            }
        }
    }

    async fn start_batch_item(&mut self, index: usize) -> Result<(), AppError> {
        let options = self.batch.options.clone().ok_or(BatchError::EmptyQueue)?;
        let path = self.batch.status.items[index].path.clone();
        self.open_path_for_batch(path).await?;
        self.auto_curve.pending = false;
        let line = self.review_positions(options.scope)?;
        self.start_review(line, options.visits, WholeGameSource::Manual, options.force)
            .await
    }

    fn copy_batch_progress(&mut self) {
        if let Some(index) = self.batch.status.current_index {
            self.batch.status.items[index].state = BatchItemState::Running {
                progress: self.review.progress.clone(),
            };
        }
    }

    fn finish_batch_item(&mut self, state: BatchItemState) {
        if let Some(index) = self.batch.status.current_index.take() {
            self.batch.status.items[index].state = state;
        }
        self.publish_batch();
    }

    /// Observe the ordinary whole-game run after each actor event. Successful
    /// completion is saved before the next file can replace the document.
    pub(super) async fn sync_batch_progress(&mut self) {
        if !self.batch_active() {
            return;
        }
        if let Some(reason) = self.batch_engine_error() {
            self.stop_batch(Some(reason)).await;
            return;
        }
        if self.batch.status.current_index.is_none() {
            return;
        }
        let before = self.batch.status.clone();
        self.copy_batch_progress();
        match self.review.progress.as_ref().map(|p| p.status.clone()) {
            Some(WholeGameStatus::Completed) if self.batch.status.phase == BatchPhase::Running => {
                let state = match self.save_batch_output().await {
                    Ok(output) => BatchItemState::Completed { output },
                    Err(error) => BatchItemState::Failed {
                        reason: error.to_string(),
                    },
                };
                self.finish_batch_item(state);
            }
            Some(WholeGameStatus::Error { reason }) => {
                self.finish_batch_item(BatchItemState::Failed { reason })
            }
            Some(WholeGameStatus::Cancelled) => {
                self.stop_batch(Some("whole-game analysis was cancelled".into()))
                    .await
            }
            _ => {
                if self.batch.status != before {
                    self.publish_batch();
                }
            }
        }
    }

    async fn save_batch_output(&mut self) -> Result<PathBuf, AppError> {
        let options = self.batch.options.as_ref().ok_or(BatchError::EmptyQueue)?;
        let index = self
            .batch
            .status
            .current_index
            .ok_or(BatchError::EmptyQueue)?;
        let source = std::path::absolute(&self.batch.status.items[index].path)?;
        let directory = options.output.directory.clone().unwrap_or_else(|| {
            source
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf()
        });
        let stamp = self.config.batch_clock.local_timestamp();
        if stamp.len() != 14 || !stamp.bytes().all(|b| b.is_ascii_digit()) {
            return Err(BatchError::InvalidTimestamp.into());
        }
        let mut name = source.file_stem().unwrap_or_default().to_os_string();
        name.push(format!("_{}_{}", options.output.suffix, stamp));
        let (tree, warnings) = self.export_document_with_analysis()?;
        let bytes = sgf::serialize(&sgf::Collection {
            games: vec![tree],
            encoding: "UTF-8".into(),
            warnings: Vec::new(),
        })
        .into_bytes();
        let mut number = 1_u64;
        loop {
            let mut filename = name.clone();
            if number > 1 {
                filename.push(format!("-{number}"));
            }
            filename.push(".sgf");
            let path = std::path::absolute(directory.join(filename))?;
            match crate::storage::write_new(path.clone(), bytes.clone()).await {
                Ok(()) => {
                    self.file_name = path.file_name().map(|v| v.to_string_lossy().into_owned());
                    self.file_state.mark_saved(&self.document, path.clone());
                    self.file_state.save_warnings = warnings;
                    self.publish_document()?;
                    return Ok(path);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    number = number.checked_add(1).ok_or_else(|| {
                        std::io::Error::other("batch output collision limit reached")
                    })?;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

fn waiting(item: &BatchItem) -> bool {
    item.state == BatchItemState::Waiting
}

fn blocked_input(input: &Input) -> bool {
    match input {
        Input::Edit(document::Command::Step(_) | document::Command::GoTo(_)) => false,
        Input::OpenBytes(_)
        | Input::OpenPath(_)
        | Input::OpenDroppedPaths(_)
        | Input::Save(_)
        | Input::PasteSgf
        | Input::NewGame(_)
        | Input::ConvertToStartPosition
        | Input::StartTrial
        | Input::EndTrial
        | Input::Edit(_)
        | Input::Undo
        | Input::Redo
        | Input::ClearAnalysis(_)
        | Input::SetAutoQuickAnalyzeOnLoad(_)
        | Input::GenerateTsumego(_)
        | Input::ExitTsumego
        | Input::EnsureNoBatch => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
