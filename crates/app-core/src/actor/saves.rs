//! Save slots and the autosave clock share the document actor's ownership.
use super::Actor;
use crate::{AppError, GamePhase, SaveSlot, save_slots_api::Request, saves::SlotStore};
use document::{Command, GameDocument};

pub(super) struct State {
    saved_content: sgf::GameTree,
}

impl Default for State {
    fn default() -> Self {
        Self {
            saved_content: GameDocument::new().to_sgf(),
        }
    }
}

impl Actor {
    fn slot_store(&self) -> SlotStore {
        SlotStore::new(
            self.config
                .cache_directory
                .as_ref()
                .unwrap_or(&self.directory)
                .clone(),
        )
    }

    pub(super) async fn save_slots_command(&mut self, request: Request) {
        let store = self.slot_store();
        // Keep reads available during update installation; mutations follow the
        // same freeze as document/settings commands.
        if self.updates.install_pending && !matches!(request, Request::List(_)) {
            let error = Err(crate::updates::UpdateError::Busy.into());
            match request {
                Request::Create(_, reply)
                | Request::Overwrite(_, reply)
                | Request::Rename(_, _, reply) => {
                    let _ = reply.send(error);
                }
                Request::Delete(_, reply) | Request::DeleteAll(reply) => {
                    let _ = reply.send(Err(crate::updates::UpdateError::Busy.into()));
                }
                Request::List(_) => {}
            }
            return;
        }
        match request {
            Request::List(reply) => {
                let _ = reply.send(Ok(store.list().await));
            }
            Request::Create(name, reply) => {
                let result = async {
                    self.ensure_no_batch()?;
                    Ok(store
                        .create(
                            name,
                            self.slot_default_name(),
                            &self.document,
                            self.slot_bytes()?,
                        )
                        .await?)
                }
                .await;
                let _ = reply.send(result);
            }
            Request::Overwrite(id, reply) => {
                let result = async {
                    self.ensure_no_batch()?;
                    self.ensure_no_move_trial()?;
                    Ok(store
                        .overwrite(&id, &self.document, self.slot_bytes()?)
                        .await?)
                }
                .await;
                let _ = reply.send(result);
            }
            Request::Rename(id, name, reply) => {
                let _ = reply.send(store.rename(&id, name).await.map_err(Into::into));
            }
            Request::Delete(id, reply) => {
                let _ = reply.send(store.delete(&id).await.map_err(Into::into));
            }
            Request::DeleteAll(reply) => {
                let _ = reply.send(store.delete_all().await.map_err(Into::into));
            }
        }
    }

    fn slot_bytes(&self) -> Result<Vec<u8>, AppError> {
        let (tree, _) = self.export_document_with_analysis()?;
        Ok(sgf::serialize(&sgf::Collection {
            games: vec![tree],
            encoding: "UTF-8".into(),
            warnings: Vec::new(),
        })
        .into_bytes())
    }

    fn slot_default_name(&self) -> String {
        if let Some(name) = &self.file_name {
            return name.clone();
        }
        let Ok(info) = self.document.game_info() else {
            return "LizzieYzy.sgf".into();
        };
        let black = info.black_name.as_deref().unwrap_or_default();
        let white = info.white_name.as_deref().unwrap_or_default();
        if black.is_empty() && white.is_empty() {
            "LizzieYzy.sgf".into()
        } else {
            format!("{black}_Vs_{white}.sgf")
        }
    }

    pub(super) async fn load_slot(&mut self, id: &str, quiet: bool) -> Result<(), AppError> {
        let (path, slot) = self.slot_store().load(id).await?;
        self.open_path_with_slot(path, Some(slot), quiet).await
    }

    pub(super) async fn set_save_setting(
        &mut self,
        enabled: bool,
        restore: bool,
    ) -> Result<(), AppError> {
        let mut settings = self.settings.clone();
        let value = if restore {
            &mut settings.restore_last_game
        } else {
            &mut settings.autosave
        };
        if *value != enabled {
            *value = enabled;
            settings.save(&self.directory).await?;
            self.settings = settings;
        }
        Ok(())
    }

    pub(super) async fn restore_last_game(&mut self) {
        if !self.settings.restore_last_game {
            return;
        }
        match self.load_slot(crate::saves::AUTOSAVE_ID, true).await {
            Ok(()) => {
                self.saves.saved_content = self
                    .file_state
                    .analysis_edits
                    .effective_tree(&self.document)
            }
            // Optional recovery must not publish OpenFailed or engine failure.
            Err(error) => tracing::warn!(%error, "last game recovery unavailable"),
        }
    }

    pub(super) async fn autosave(&mut self, exiting: bool) {
        if !self.settings.autosave
            || self.batch_active()
            || self.match_active()
            || self.coach.is_starting()
            || self.coach.session_active()
            || self.game.as_ref().is_some_and(|game| {
                matches!(game.view.phase, GamePhase::Playing | GamePhase::Paused)
            })
            || self.move_trial.is_some()
            || self.tsumego.is_some()
            || self.updates.install_pending
        {
            return;
        }
        // Analysis frames and navigation do not change this content identity.
        // Use the effective tree so explicit clear-analysis edits do count.
        let content = self
            .file_state
            .analysis_edits
            .effective_tree(&self.document);
        if !exiting && content == self.saves.saved_content {
            return;
        }
        // Neither an untouched startup nor a default new game should erase the
        // previous session. Other exit saves still record navigation-only changes.
        if exiting
            && (content == GameDocument::new().to_sgf()
                || GameDocument::new_game(crate::NewGameOptions::default().into())
                    .is_ok_and(|empty| content == empty.to_sgf()))
        {
            return;
        }
        let result = async {
            self.slot_store()
                .autosave(self.slot_default_name(), &self.document, self.slot_bytes()?)
                .await?;
            Ok::<_, AppError>(())
        }
        .await;
        match result {
            Ok(()) => self.saves.saved_content = content,
            Err(error) => tracing::warn!(%error, "automatic save failed"),
        }
    }
}

/// Child indices, unlike arena IDs, survive SGF serialization. A stale path
/// falls back to the same move number on the main line, then its final node.
pub(super) fn restore_node(document: &mut GameDocument, slot: &SaveSlot) -> Result<(), AppError> {
    let exact = slot
        .node_path
        .iter()
        .try_fold(document.root(), |id, &index| {
            document.node(id).ok()?.children().get(index).copied()
        });
    let node = match exact {
        Some(node) => node,
        None => {
            let numbers: std::collections::HashMap<_, _> = document
                .variation_tree()?
                .into_iter()
                .map(|node| (node.id, node.move_number))
                .collect();
            let mut current = document.root();
            loop {
                if numbers.get(&current) == Some(&slot.move_number) {
                    break current;
                }
                match document.node(current)?.children().first() {
                    Some(child) => current = *child,
                    None => break current,
                }
            }
        }
    };
    document.apply(Command::GoTo(node))?;
    Ok(())
}
