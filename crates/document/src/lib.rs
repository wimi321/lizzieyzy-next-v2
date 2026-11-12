//! Pure game document tree and editing commands.

mod edit;
mod game;
mod key;
pub mod lz;
mod markup;
mod metadata;
mod move_editing;
mod presentation;
mod preview;
mod semantics;
mod setup;
mod tree;
mod types;

pub use game::{GameInfoEdit, NewGameOptions};
pub use key::AnalysisKey;
pub use lz::{LzError, SavedAnalysis, SavedCandidate};
pub use markup::{Markup, MarkupKind};
pub use move_editing::StoneOrigin;
pub use presentation::{GameInfo, VariationNode};
pub use preview::{PreviewStone, VariationPreview};
pub use tree::GameDocument;
pub use types::{
    Captures, Command, DocError, DocumentNode, DocumentSnapshot, DocumentWarning, GamePosition,
    NodeId, Revision, WarningKind,
};

#[cfg(test)]
mod tests;
