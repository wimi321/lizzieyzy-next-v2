//! Frozen evidence and document edits belong to the app actor; HTTP does not.
use super::Actor;
use crate::commentary::{Connection, Generation, Request};
use crate::commentary_keys::{CommentaryKeyStore, CommentaryKeys};
use crate::{
    AppError, AppEvent, CommentaryError, CommentaryMode, CommentaryPhase, CommentaryRequest,
    CommentarySettings, CommentaryUpdated,
};
use document::NodeId;
use std::{future::pending, sync::Arc};
use tokio::{sync::watch, task::JoinHandle};

mod evidence;

pub(super) struct State {
    keys: CommentaryKeys,
    active: Option<Active>,
    pub previous: Option<Previous>,
    view: CommentaryUpdated,
    sequence: u32,
}
struct Active {
    task: JoinHandle<()>,
    updates: watch::Receiver<CommentaryUpdated>,
    context: Previous,
    follow_up: Option<String>,
    write: bool,
}
impl Drop for Active {
    fn drop(&mut self) {
        self.task.abort();
    }
}
#[derive(Clone)]
pub(super) struct Previous {
    evidence: ::commentary::Evidence,
    answer: String,
    mode: CommentaryMode,
    target: NodeId,
    content_revision: u64,
    model: String,
    block: String,
    verified: bool,
}
impl State {
    pub fn new(store: Option<Arc<dyn CommentaryKeyStore>>, options: &CommentarySettings) -> Self {
        Self {
            keys: CommentaryKeys::with_address(
                store,
                options.base_url.clone(),
                options.remember_key,
            ),
            active: None,
            previous: None,
            view: Default::default(),
            sequence: 0,
        }
    }
}
pub(super) async fn receive(state: &mut State) -> CommentaryUpdated {
    let Some(active) = state.active.as_mut() else {
        return pending().await;
    };
    if active.updates.changed().await.is_err() {
        return pending().await;
    }
    active.updates.borrow_and_update().clone()
}

impl Actor {
    pub(super) async fn commentary_command(&mut self, request: Request) {
        match request {
            Request::Start(request, reply) => {
                let _ = reply.send(self.start_commentary(request).await);
            }
            Request::Stop(reply) => {
                self.stop_commentary();
                let _ = reply.send(self.commentary.view.clone());
            }
            Request::Status(reply) => {
                let _ = reply.send(self.commentary.view.clone());
            }
            Request::Settings(settings, reply) => {
                let _ = reply.send(self.set_commentary_settings(settings).await);
            }
            Request::SetKey(key, remember, reply) => {
                let _ = reply.send(self.set_commentary_key(key, remember).await);
            }
            Request::ClearKey(reply) => {
                self.stop_commentary();
                let status = self
                    .commentary
                    .keys
                    .clear(&self.settings.commentary.base_url)
                    .await;
                let _ = reply.send(Ok(status));
            }
            Request::KeyStatus(reply) => {
                let settings = &self.settings.commentary;
                let status = self
                    .commentary
                    .keys
                    .configure(&settings.base_url, settings.remember_key)
                    .await;
                let _ = reply.send(status);
            }
            Request::Connection(reply) => {
                let _ = reply.send(self.commentary_connection().await);
            }
        }
    }

    async fn set_commentary_settings(
        &mut self,
        options: CommentarySettings,
    ) -> Result<CommentarySettings, AppError> {
        let options = options.normalized()?;
        let mut settings = self.settings.clone();
        settings.commentary = options.clone();
        settings.save(&self.directory).await?;
        self.stop_commentary();
        self.commentary
            .keys
            .configure(&options.base_url, options.remember_key)
            .await;
        self.settings = settings;
        Ok(options)
    }
    async fn set_commentary_key(
        &mut self,
        key: String,
        remember: bool,
    ) -> Result<crate::CommentaryKeyStatus, AppError> {
        // Validate before changing persistent preferences or existing credentials.
        if self.settings.commentary.base_url.is_empty() {
            return Err(CommentaryError::NotConfigured.into());
        }
        crate::commentary_client::validate_key(&key).map_err(CommentaryError::from)?;
        let mut settings = self.settings.clone();
        settings.commentary.remember_key = remember;
        settings.save(&self.directory).await?;
        let status = self
            .commentary
            .keys
            .set(&settings.commentary.base_url, key, remember)
            .await
            .map_err(CommentaryError::from)?;
        self.settings = settings;
        Ok(status)
    }
    async fn commentary_connection(&mut self) -> Result<Connection, AppError> {
        let settings = &self.settings.commentary;
        if settings.base_url.is_empty() {
            return Err(CommentaryError::NotConfigured.into());
        }
        self.commentary
            .keys
            .configure(&settings.base_url, settings.remember_key)
            .await;
        let key = self.commentary.keys.key().unwrap_or_default().to_owned();
        crate::commentary_client::validate_key(&key).map_err(CommentaryError::from)?;
        let client = crate::commentary_client::CommentaryClient::new(&settings.base_url)
            .map_err(CommentaryError::from)?;
        Ok(Connection {
            client,
            key,
            model: settings.model.clone(),
        })
    }

    async fn start_commentary(
        &mut self,
        request: CommentaryRequest,
    ) -> Result<CommentaryUpdated, AppError> {
        if self.commentary.active.is_some() {
            return Err(CommentaryError::Busy.into());
        }
        self.ensure_no_batch()?;
        if self.game_playing() || self.match_active() || self.updates.install_pending {
            return Err(CommentaryError::Busy.into());
        }
        if self.settings.commentary.model.is_empty() {
            return Err(CommentaryError::NotConfigured.into());
        }
        let language = crate::commentary::language(self.settings.language);
        let question = request
            .follow_up
            .as_deref()
            .filter(|s| !s.trim().is_empty());
        if request.follow_up.is_some() && question.is_none() {
            return Err(CommentaryError::InvalidSettings("follow_up".into()).into());
        }
        let context = if question.is_some() {
            let previous = self
                .commentary
                .previous
                .as_ref()
                .ok_or(CommentaryError::NoPrevious)?;
            if previous.content_revision != self.document.content_revision() {
                return Err(CommentaryError::PositionChanged.into());
            }
            previous.clone()
        } else {
            let (evidence, target) = self.commentary_evidence(&request.mode, language)?;
            Previous {
                evidence,
                answer: String::new(),
                mode: request.mode,
                target,
                content_revision: self.document.content_revision(),
                model: self.settings.commentary.model.clone(),
                block: String::new(),
                verified: true,
            }
        };
        let mode = match context.mode {
            CommentaryMode::NextMove => ::commentary::Mode::NextMove,
            CommentaryMode::Range { .. } => ::commentary::Mode::Range,
            CommentaryMode::WholeGame => ::commentary::Mode::WholeGame,
        };
        let follow_up = question.map(|question| ::commentary::FollowUp {
            previous_answer: &context.answer,
            question,
        });
        let messages = ::commentary::build_prompt(
            &context.evidence,
            mode,
            language,
            &self.settings.commentary.preferences(),
            follow_up,
        )
        .map_err(crate::commentary::domain_error)?;
        let connection = self.commentary_connection().await?;
        let mut context = context;
        context.model = connection.model.clone();
        let id = self
            .commentary
            .sequence
            .checked_add(1)
            .ok_or(AppError::RevisionLimit)?;
        let view = CommentaryUpdated {
            request_id: id,
            phase: CommentaryPhase::Preparing,
            evidence: context.evidence.text.clone(),
            positions: context
                .evidence
                .positions
                .iter()
                .map(|p| crate::CommentaryPosition {
                    node_id: p.node_id,
                    move_number: p.move_number,
                })
                .collect(),
            analyzed: context.evidence.analyzed as u32,
            selected: context.evidence.selected as u32,
            omitted: context.evidence.omitted as u32,
            ..Default::default()
        };
        let (tx, rx) = watch::channel(view.clone());
        let job = Generation {
            connection,
            evidence: context.evidence.clone(),
            messages,
            language,
            view: view.clone(),
        };
        let task = tokio::spawn(crate::commentary::generate(job, tx));
        self.commentary.sequence = id;
        self.commentary.active = Some(Active {
            task,
            updates: rx,
            context,
            follow_up: question.map(str::to_owned),
            write: self.settings.commentary.write_to_comment,
        });
        self.publish_commentary(view.clone());
        Ok(view)
    }

    pub(super) fn stop_commentary(&mut self) {
        if self.commentary.active.take().is_some() {
            let mut view = self.commentary.view.clone();
            view.phase = CommentaryPhase::Stopped;
            view.delta.clear();
            self.publish_commentary(view);
        }
    }
    pub(super) fn reset_commentary(&mut self) {
        if self.commentary.sequence == 0 {
            return;
        }
        self.stop_commentary();
        self.commentary.previous = None;
        let view = CommentaryUpdated {
            request_id: self.commentary.sequence,
            ..Default::default()
        };
        self.publish_commentary(view);
    }
    fn publish_commentary(&mut self, view: CommentaryUpdated) {
        self.commentary.view = view.clone();
        self.output.send(AppEvent::CommentaryUpdated(view));
    }
    pub(super) async fn commentary_progress(&mut self, mut view: CommentaryUpdated) {
        if self.commentary.active.is_none() || view.request_id != self.commentary.view.request_id {
            return;
        }
        if matches!(
            view.phase,
            CommentaryPhase::Completed | CommentaryPhase::Failed
        ) && let Some(mut active) = self.commentary.active.take()
            && view.phase == CommentaryPhase::Completed
        {
            if active.follow_up.is_some() && !active.context.verified {
                view.issues.push(
                    "The previous answer was not verified; this conversation is not saved.".into(),
                );
            }
            if view.issues.is_empty() {
                let timestamp = chrono::Utc::now().to_rfc3339();
                let block = if let Some(question) = &active.follow_up {
                    ::commentary::codec::append_follow_up(
                        &active.context.block,
                        question,
                        &view.text,
                        &active.context.model,
                        &timestamp,
                    )
                } else {
                    ::commentary::codec::encode(&view.text, &active.context.model, &timestamp)
                };
                match block {
                    Ok(block) => active.context.block = block,
                    Err(error) => {
                        view.error = Some(crate::commentary::domain_error(error));
                        view.phase = CommentaryPhase::Failed;
                    }
                }
            }
            if view.phase == CommentaryPhase::Completed && active.write && view.issues.is_empty() {
                if let Err(error) = self.write_commentary(&active).await {
                    view.error = Some(error);
                    view.phase = CommentaryPhase::Failed;
                } else {
                    active.context.content_revision = self.document.content_revision();
                }
            }
            if view.phase == CommentaryPhase::Completed {
                active.context.answer = view.text.clone();
                active.context.verified = view.issues.is_empty();
                self.commentary.previous = Some(active.context.clone());
            }
        }
        self.publish_commentary(view);
    }
    async fn write_commentary(&mut self, active: &Active) -> Result<(), CommentaryError> {
        if self.document.content_revision() != active.context.content_revision
            || self.batch_active()
            || self.game_playing()
            || self.match_active()
        {
            return Err(CommentaryError::PositionChanged);
        }
        let target = active.context.target;
        let original = self
            .document
            .node(target)
            .map_err(|_| CommentaryError::PositionChanged)?
            .comment()
            .unwrap_or_default();
        let block = ::commentary::codec::decode(&active.context.block)
            .ok_or(CommentaryError::InvalidEvidence)?;
        let comment =
            ::commentary::codec::upsert(original, &block.body, &block.model, &block.generated_at)
                .map_err(crate::commentary::domain_error)?;
        let current = self.document.current();
        self.next_revision()
            .map_err(|_| CommentaryError::WriteFailed)?;
        self.document
            .apply(document::Command::GoTo(target))
            .map_err(|_| CommentaryError::PositionChanged)?;
        let before = self.document.content_revision();
        let result = self.document.apply(document::Command::SetComment(comment));
        let restore = self.document.apply(document::Command::GoTo(current));
        result
            .and(restore)
            .map_err(|_| CommentaryError::WriteFailed)?;
        if before != self.document.content_revision() {
            self.file_state.analysis_edits.document_edit();
        }
        self.document_changed()
            .await
            .map_err(|_| CommentaryError::WriteFailed)
    }
}

#[cfg(test)]
mod tests;
