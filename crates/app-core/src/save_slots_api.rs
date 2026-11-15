//! Slot commands are serialized by the document's existing actor.
use crate::{
    AppCore, AppError, SaveSlot,
    actor::{Command, Input},
};
use tokio::sync::oneshot;

pub(crate) enum Request {
    List(oneshot::Sender<Result<Vec<SaveSlot>, AppError>>),
    Create(String, oneshot::Sender<Result<SaveSlot, AppError>>),
    Overwrite(String, oneshot::Sender<Result<SaveSlot, AppError>>),
    Rename(String, String, oneshot::Sender<Result<SaveSlot, AppError>>),
    Delete(String, oneshot::Sender<Result<(), AppError>>),
    DeleteAll(oneshot::Sender<Result<(), AppError>>),
}

impl AppCore {
    pub async fn save_slots(&self) -> Result<Vec<SaveSlot>, AppError> {
        self.slot_request(Request::List).await
    }

    pub async fn save_to_new_slot(&self, name: String) -> Result<SaveSlot, AppError> {
        self.slot_request(|reply| Request::Create(name, reply))
            .await
    }

    pub async fn overwrite_slot(&self, id: String) -> Result<SaveSlot, AppError> {
        self.slot_request(|reply| Request::Overwrite(id, reply))
            .await
    }

    pub async fn rename_slot(&self, id: String, name: String) -> Result<SaveSlot, AppError> {
        self.slot_request(|reply| Request::Rename(id, name, reply))
            .await
    }

    /// Like open_path, the caller is responsible for the unsaved-change prompt.
    pub async fn load_slot(&self, id: String) -> Result<u64, AppError> {
        self.send(Input::LoadSlot(id)).await
    }

    pub async fn delete_slot(&self, id: String) -> Result<(), AppError> {
        self.slot_request(|reply| Request::Delete(id, reply)).await
    }

    pub async fn delete_all_slots(&self) -> Result<(), AppError> {
        self.slot_request(Request::DeleteAll).await
    }

    pub async fn set_autosave(&self, enabled: bool) -> Result<bool, AppError> {
        self.send(Input::SetAutosave(enabled)).await?;
        Ok(enabled)
    }

    pub async fn set_restore_last_game(&self, enabled: bool) -> Result<bool, AppError> {
        self.send(Input::SetRestoreLastGame(enabled)).await?;
        Ok(enabled)
    }

    async fn slot_request<T>(
        &self,
        request: impl FnOnce(oneshot::Sender<Result<T, AppError>>) -> Request,
    ) -> Result<T, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::SaveSlots(request(reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}
