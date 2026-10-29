//! Pure, synchronous types for KataGo's line-delimited analysis JSON protocol.
//!
//! Numeric evaluations are passed through unchanged. Configure the engine with
//! `reportAnalysisWinratesAs = BLACK` before exposing them to the application.

mod request;
mod response;

pub use request::{Action, AnalysisQuery, Player, Request, Rules};
pub use response::{
    ActionResponse, AnalysisResponse, EngineLine, ErrorLine, MoveInfo, NoResultsResponse, RootInfo,
    WarningLine,
};

use serde_json::Value;

/// A malformed line or an unsupported response shape.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("invalid KataGo JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid KataGo response: {0}")]
    InvalidResponse(&'static str),
}

/// Parse one complete stdout line. Unknown fields are ignored at every level.
///
/// This validates the response envelope, including the special cancellation
/// result, rather than interpreting an incomplete result as an empty analysis.
pub fn parse_line(line: &str) -> Result<EngineLine, ProtocolError> {
    let value: Value = serde_json::from_str(line)?;
    let object = value
        .as_object()
        .ok_or(ProtocolError::InvalidResponse("expected a JSON object"))?;
    if object.contains_key("error") {
        return Ok(EngineLine::Error(serde_json::from_value(value)?));
    }
    if object.contains_key("warning") {
        return Ok(EngineLine::Warning(serde_json::from_value(value)?));
    }
    if object.contains_key("action") {
        return Ok(EngineLine::Action(serde_json::from_value(value)?));
    }
    if let Some(no_results) = object.get("noResults") {
        match no_results.as_bool() {
            Some(true) => {
                if object.contains_key("rootInfo") || object.contains_key("moveInfos") {
                    return Err(ProtocolError::InvalidResponse(
                        "noResults must not contain analysis data",
                    ));
                }
                let response: NoResultsResponse = serde_json::from_value(value)?;
                if response.is_during_search {
                    return Err(ProtocolError::InvalidResponse("noResults must be final"));
                }
                return Ok(EngineLine::NoResults(response));
            }
            Some(false) => {}
            None => return Err(ProtocolError::InvalidResponse("noResults must be boolean")),
        }
    }
    Ok(EngineLine::Analysis(Box::new(serde_json::from_value(
        value,
    )?)))
}

#[cfg(test)]
mod tests;
