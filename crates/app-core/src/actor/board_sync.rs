use super::Actor;
use crate::{
    AppError, AppEvent, BoardSyncError, BoardSyncOptions, BoardSyncPhase, BoardSyncStatus,
    board_sync::{Request, process::Process, reconcile::Reconciler},
};
use document::NodeId;
use std::{future::pending, path::PathBuf};

#[derive(Default)]
pub(super) struct State {
    process: Option<Process>,
    program: Option<PathBuf>,
    options: BoardSyncOptions,
    decoder: readboard::Decoder,
    reconciler: Reconciler,
    pub status: BoardSyncStatus,
    pub resolved_node: Option<NodeId>,
    pub analysis_paused: bool,
}

pub(super) async fn receive(state: &mut State) -> Result<Option<String>, BoardSyncError> {
    match &mut state.process {
        Some(process) => process.next_line().await,
        None => pending().await,
    }
}

impl Actor {
    pub(super) fn sync_active(&self) -> bool {
        self.board_sync.status.phase == BoardSyncPhase::Syncing
    }

    pub(super) fn pondering(&self) -> bool {
        if self.sync_active() {
            !self.board_sync.analysis_paused
        } else {
            self.settings.pondering
        }
    }

    pub(super) fn publish_sync(&self) {
        self.output.send(AppEvent::BoardSyncStatusChanged(
            self.board_sync.status.clone(),
        ));
    }

    pub(super) async fn board_sync_command(
        &mut self,
        request: Request,
    ) -> Result<BoardSyncStatus, AppError> {
        match request {
            Request::Status => {}
            Request::Stop => self.stop_board_sync().await?,
            Request::Start(program, options) => {
                if self.board_sync.process.is_some()
                    && self.board_sync.program.as_ref() == Some(&program)
                    && self.board_sync.options == options
                {
                    return Ok(self.board_sync.status.clone());
                }
                // Validate and launch first, preserving a working session on failure.
                let process = Process::spawn(program.clone(), &options).await?;
                if let Err(error) = self.stop_board_sync().await {
                    let _ = process.stop().await;
                    return Err(error);
                }
                self.board_sync = State {
                    process: Some(process),
                    program: Some(program),
                    status: BoardSyncStatus {
                        phase: BoardSyncPhase::Starting,
                        always_sync: options.always_sync,
                        always_goto_last: options.always_goto_last,
                        remote_move_number: None,
                        error: None,
                    },
                    options,
                    ..State::default()
                };
                self.publish_sync();
            }
        }
        Ok(self.board_sync.status.clone())
    }

    pub(super) async fn stop_board_sync(&mut self) -> Result<(), AppError> {
        if self.board_sync.process.is_none()
            && self.board_sync.status.phase == BoardSyncPhase::Stopped
        {
            return Ok(());
        }
        let result = if let Some(process) = self.board_sync.process.take() {
            process.stop().await
        } else {
            Ok(())
        };
        self.board_sync.resolved_node = None;
        self.board_sync.reconciler = Reconciler::default();
        self.board_sync.decoder = readboard::Decoder::default();
        self.board_sync.status.phase = if result.is_ok() {
            BoardSyncPhase::Stopped
        } else {
            BoardSyncPhase::Failed
        };
        self.board_sync.status.error = result.as_ref().err().cloned();
        self.board_sync.status.remote_move_number = None;
        self.output.clear_analysis();
        self.publish_sync();
        self.update_focus().await;
        result.map_err(AppError::from)
    }

    pub(super) async fn board_sync_line(&mut self, result: Result<Option<String>, BoardSyncError>) {
        let line = match result {
            Ok(Some(line)) => line,
            other => {
                let error = other.err().unwrap_or(BoardSyncError::Process {
                    reason: "readboard closed its output".into(),
                });
                let _ = self.stop_board_sync().await;
                self.board_sync.status.phase = BoardSyncPhase::Failed;
                self.board_sync.status.error = Some(error);
                self.publish_sync();
                return;
            }
        };
        match self.board_sync.decoder.push_line(&line) {
            Ok(Some(event)) => {
                if let Err(error) = self.board_sync_event(event).await {
                    let error = match error {
                        AppError::BoardSync(error) => error,
                        error => BoardSyncError::Protocol {
                            reason: error.to_string(),
                        },
                    };
                    if matches!(error, BoardSyncError::Process { .. }) {
                        let _ = self.stop_board_sync().await;
                        self.board_sync.status.phase = BoardSyncPhase::Failed;
                    }
                    self.board_sync.status.error = Some(error);
                    self.publish_sync();
                }
            }
            Ok(None) => {}
            Err(error) => {
                // The decoder discards the bad frame. A subsequent clear/start
                // can recover without applying any partial board.
                self.board_sync.status.error = Some(BoardSyncError::Protocol {
                    reason: error.to_string(),
                });
                self.publish_sync();
            }
        }
    }

    async fn board_sync_event(&mut self, event: readboard::Event) -> Result<(), AppError> {
        match event {
            readboard::Event::Ready => {
                let running = self.pondering() && self.running.is_some();
                if let Some(process) = &mut self.board_sync.process {
                    if process.ready {
                        return Ok(());
                    }
                    process.send(readboard::Command::Version).await?;
                    process
                        .send(readboard::Command::AnalysisState { running })
                        .await?;
                    process.ready = true;
                }
                self.board_sync.status.phase = BoardSyncPhase::Ready;
                self.publish_sync();
            }
            readboard::Event::Sync => {
                self.board_sync.analysis_paused = false;
                self.board_sync.status.phase = BoardSyncPhase::Syncing;
                self.publish_sync();
                self.update_focus().await;
            }
            readboard::Event::PauseAnalysis | readboard::Event::ResumeAnalysis => {
                self.board_sync.analysis_paused = matches!(event, readboard::Event::PauseAnalysis);
                self.output.clear_analysis();
                self.update_focus().await;
                let running = self.pondering() && self.running.is_some();
                if let Some(process) = &mut self.board_sync.process {
                    process
                        .send(readboard::Command::AnalysisState { running })
                        .await?;
                }
            }
            readboard::Event::Ended => self.stop_board_sync().await?,
            readboard::Event::Stopped => {
                self.board_sync.status.phase = BoardSyncPhase::Ready;
                self.board_sync.resolved_node = None;
                self.board_sync.status.error = None;
                self.publish_sync();
                self.output.clear_analysis();
                self.update_focus().await;
            }
            readboard::Event::Snapshot(snapshot) => {
                self.next_revision()?;
                let mut document = self.document.clone();
                let outcome = self.board_sync.reconciler.apply(
                    &mut document,
                    &snapshot,
                    self.board_sync.options.always_sync,
                    self.board_sync.options.always_goto_last,
                )?;
                if outcome.held {
                    return Ok(());
                }
                self.board_sync.status.phase = BoardSyncPhase::Syncing;
                self.board_sync.status.remote_move_number = snapshot.fox_move_number;
                self.board_sync.status.error = None;
                self.board_sync.resolved_node = Some(outcome.resolved_node);
                if outcome.rebuilt {
                    self.replace_document(document).await?;
                } else if outcome.changed {
                    self.document = document;
                    self.document_changed().await?;
                } else {
                    self.update_focus().await;
                }
                self.publish_sync();
                let running = self.pondering() && self.running.is_some();
                if let Some(process) = &mut self.board_sync.process {
                    process
                        .send(readboard::Command::AnalysisState { running })
                        .await?;
                }
            }
            readboard::Event::ClearBoard => {
                self.board_sync.reconciler = Reconciler::default();
                self.board_sync.resolved_node = None;
                self.board_sync.status.remote_move_number = None;
                let size = self
                    .document
                    .position_at(self.document.root())?
                    .board
                    .size();
                let tree = sgf::GameTree {
                    sequence: vec![sgf::Node {
                        properties: vec![sgf::Property {
                            identifier: "SZ".into(),
                            values: vec![format!("{}:{}", size.width, size.height)],
                        }],
                    }],
                    children: Vec::new(),
                };
                self.replace_document(document::GameDocument::from_sgf(&tree)?)
                    .await?;
                self.publish_sync();
            }
            readboard::Event::Error(reason) => {
                return Err(BoardSyncError::Process { reason }.into());
            }
            // Exchange-order belongs to helper play controls. Never turn it
            // into a fictitious pass in the recorded game.
            readboard::Event::ExchangeTurn
            | readboard::Event::Version(_)
            | readboard::Event::Started(_) => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
