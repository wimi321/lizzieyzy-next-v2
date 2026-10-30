//! Pure game document tree and editing commands.

mod edit;
mod key;
mod metadata;
mod presentation;
mod preview;
mod semantics;
mod tree;
mod types;

pub use key::AnalysisKey;
pub use presentation::{GameInfo, VariationNode};
pub use preview::{PreviewStone, VariationPreview};
pub use tree::GameDocument;
pub use types::{
    Captures, Command, DocError, DocumentNode, DocumentSnapshot, DocumentWarning, GamePosition,
    NodeId, Revision, WarningKind,
};

#[cfg(test)]
mod tests;
