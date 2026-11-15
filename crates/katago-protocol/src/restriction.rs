//! Typed restriction payloads. The legacy session request envelope still owns
//! a generic extension map, so the adapter is isolated here until that envelope
//! can change together with all of its callers.
use crate::{AnalysisQuery, Player};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRestriction {
    pub player: Player,
    pub moves: Vec<String>,
    pub until_depth: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRestrictions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_moves: Option<Vec<MoveRestriction>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avoid_moves: Option<Vec<MoveRestriction>>,
}

#[derive(Debug, thiserror::Error)]
pub enum RestrictionError {
    #[error("restriction depth must be positive")]
    InvalidDepth,
    #[error("allowMoves may specify each player only once")]
    DuplicatePlayer,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl MoveRestrictions {
    pub fn validate(&self) -> Result<(), RestrictionError> {
        if self
            .allow_moves
            .iter()
            .chain(self.avoid_moves.iter())
            .flatten()
            .any(|entry| entry.until_depth == 0)
        {
            return Err(RestrictionError::InvalidDepth);
        }
        if let Some(entries) = &self.allow_moves {
            for (index, entry) in entries.iter().enumerate() {
                if entries[..index]
                    .iter()
                    .any(|other| other.player == entry.player)
                {
                    return Err(RestrictionError::DuplicatePlayer);
                }
            }
        }
        Ok(())
    }

    pub fn from_extensions(extensions: &BTreeMap<String, Value>) -> Result<Self, RestrictionError> {
        let result = Self {
            allow_moves: extensions
                .get("allowMoves")
                .cloned()
                .map(serde_json::from_value)
                .transpose()?,
            avoid_moves: extensions
                .get("avoidMoves")
                .cloned()
                .map(serde_json::from_value)
                .transpose()?,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn write_extensions(
        &self,
        extensions: &mut BTreeMap<String, Value>,
    ) -> Result<(), RestrictionError> {
        self.validate()?;
        let allow = self
            .allow_moves
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;
        let avoid = self
            .avoid_moves
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;
        for (key, value) in [("allowMoves", allow), ("avoidMoves", avoid)] {
            if let Some(value) = value {
                extensions.insert(key.into(), value);
            } else {
                extensions.remove(key);
            }
        }
        Ok(())
    }
}

impl AnalysisQuery {
    pub fn move_restrictions(&self) -> Result<MoveRestrictions, RestrictionError> {
        MoveRestrictions::from_extensions(&self.extensions)
    }

    pub fn set_move_restrictions(
        &mut self,
        restrictions: &MoveRestrictions,
    ) -> Result<(), RestrictionError> {
        restrictions.write_extensions(&mut self.extensions)
    }
}

#[cfg(test)]
mod tests;
