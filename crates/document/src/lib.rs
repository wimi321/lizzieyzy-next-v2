//! Pure game document tree and editing commands.

mod edit;
mod key;
mod metadata;
mod semantics;
mod tree;
mod types;

pub use key::AnalysisKey;
pub use tree::GameDocument;
pub use types::{
    Captures, Command, DocError, DocumentNode, DocumentSnapshot, DocumentWarning, GamePosition,
    NodeId, Revision, WarningKind,
};

#[cfg(test)]
mod tests;
