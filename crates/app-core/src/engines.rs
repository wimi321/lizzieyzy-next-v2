use crate::{AppCore, AppError, EngineProfile, Settings, SettingsError, actor::Command};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::sync::oneshot;
use ts_rs::TS;

/// Stable identity and user-facing name are independent of profile/cache identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EngineEntry {
    #[ts(as = "Option<String>", optional)]
    pub id: String,
    #[ts(as = "Option<String>", optional)]
    pub name: String,
    #[ts(as = "Option<EngineProfile>", optional)]
    pub profile: EngineProfile,
}

pub(crate) enum Request {
    Add(String, EngineProfile),
    Update(String, String, EngineProfile),
    Remove(String),
    Reorder(Vec<String>),
    Select(String),
    Restart,
    Close,
}

impl AppCore {
    pub async fn add_engine(
        &self,
        name: String,
        profile: EngineProfile,
    ) -> Result<Settings, AppError> {
        self.engines_request(Request::Add(name, profile)).await
    }

    pub async fn update_engine(
        &self,
        id: String,
        name: String,
        profile: EngineProfile,
    ) -> Result<Settings, AppError> {
        self.engines_request(Request::Update(id, name, profile))
            .await
    }

    pub async fn remove_engine(&self, id: String) -> Result<Settings, AppError> {
        self.engines_request(Request::Remove(id)).await
    }

    pub async fn reorder_engines(&self, ids: Vec<String>) -> Result<Settings, AppError> {
        self.engines_request(Request::Reorder(ids)).await
    }

    pub async fn select_main_engine(&self, id: String) -> Result<Settings, AppError> {
        self.engines_request(Request::Select(id)).await
    }

    pub async fn restart_engine(&self) -> Result<Settings, AppError> {
        self.engines_request(Request::Restart).await
    }

    pub async fn close_engine(&self) -> Result<Settings, AppError> {
        self.engines_request(Request::Close).await
    }

    async fn engines_request(&self, request: Request) -> Result<Settings, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Engines(request, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

impl Settings {
    pub(crate) fn normalize_engines(&mut self) -> Result<(), SettingsError> {
        // Old persisted settings and source callers that still supply only engine.
        if self.engines.is_empty() && self.main_engine_id.is_none() {
            if let Some(profile) = self.engine.clone() {
                self.engines.push(EngineEntry {
                    id: "legacy-engine".into(),
                    name: "KataGo".into(),
                    profile,
                });
                self.main_engine_id = Some("legacy-engine".into());
            }
            return Ok(());
        }
        let mut ids = HashSet::new();
        for entry in &self.engines {
            if entry.id.trim().is_empty() || entry.name.trim().is_empty() || !ids.insert(&entry.id)
            {
                return Err(invalid(
                    "engine IDs and names must be nonempty; IDs must be unique",
                ));
            }
        }
        if let Some(id) = &self.main_engine_id {
            let entry = self
                .engines
                .iter()
                .find(|entry| &entry.id == id)
                .ok_or_else(|| invalid("main engine ID does not exist"))?;
            if self.engine.is_some() {
                self.engine = Some(entry.profile.clone());
            }
        } else if self.engine.is_some() {
            return Err(invalid("enabled engine needs a main engine ID"));
        }
        Ok(())
    }

    /// Temporary adapter for the existing settings UI and first-run setup.
    pub(crate) fn set_legacy_engine(&mut self, profile: Option<EngineProfile>) {
        if let Some(profile) = &profile {
            if let Some(entry) = self
                .engines
                .iter_mut()
                .find(|entry| Some(&entry.id) == self.main_engine_id.as_ref())
            {
                entry.profile = profile.clone();
            } else {
                let id = self.next_engine_id();
                self.engines.push(EngineEntry {
                    id: id.clone(),
                    name: "KataGo".into(),
                    profile: profile.clone(),
                });
                self.main_engine_id = Some(id);
            }
        }
        self.engine = profile;
    }

    pub(crate) fn next_engine_id(&self) -> String {
        let mut index = 1_u64;
        loop {
            let id = format!("engine-{index}");
            if !self.engines.iter().any(|entry| entry.id == id) {
                return id;
            }
            index += 1;
        }
    }
}

pub(crate) fn invalid(reason: &str) -> SettingsError {
    SettingsError::InvalidProfile(reason.to_owned())
}
