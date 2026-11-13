use go_core::{
    BoardSize, BoardSizeError, Color, IllegalMove, Move, Point, Position, RulePreset, SetupError,
};
use sgf::Property;

/// Stable within a document, including across deletion and undo. Never recycled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

pub type Revision = u64;

/// A recoverable metadata issue. The original SGF properties remain untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentWarning {
    pub node: NodeId,
    pub property: String,
    pub kind: WarningKind,
}

/// Structured diagnostics for a future localized UI. Fallbacks are canonical
/// values (for example "19x19", "7.5", "0", "chinese") or the property "KO".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningKind {
    FirstValueUsed,
    InvalidValue { fallback: String },
    OutOfRange { fallback: String },
    UnknownRules { fallback: String },
    IgnoredNonRoot,
    IgnoredPlayer,
    NormalizedCase,
    KomiFromKo,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Play(Move),
    /// Explicit color; full Go rules still apply, without checking whose turn.
    PlayColored(Move),
    /// Inserts after the current node and reconnects its continuations.
    InsertMove(Move),
    SwitchStone(Point),
    DeleteStone(Point),
    MoveStone {
        from: Point,
        to: Point,
    },
    /// Always creates a distinct variation and keeps the current focus.
    AddVariation(Vec<Move>),
    GoTo(NodeId),
    /// Negative: ancestors. Positive: first children. Stops at either boundary.
    Step(i32),
    /// Replaces C properties with one comment; an empty string removes them.
    SetComment(String),
    /// Deletes the whole subtree; deleting the root is forbidden.
    DeleteNode(NodeId),
    /// Makes this node the first child of its parent.
    PromoteVariation(NodeId),
    /// Deletes from the nearest ancestor fork, or this node when no fork exists.
    DeleteBranch(NodeId),
    /// Makes every ancestor on the path the first child, as one undoable edit.
    PromoteToMain(NodeId),
    SetGameInfo(crate::GameInfoEdit),
    SetSetupStone {
        point: Point,
        color: Option<Color>,
    },
    ClearSetup,
    SetFirstPlayer(Color),
    SetMarkup {
        point: Point,
        kind: crate::MarkupKind,
    },
    ClearMarkup,
    ToggleToPlay,
    /// Transforms all coordinates in the live tree as one reversible edit.
    Transform(crate::BoardTransform),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocError {
    #[error("board rotations require a square board, got {0:?}")]
    TransformRequiresSquare(BoardSize),
    #[error("there is no surviving played stone at {0:?}")]
    NoMoveAtPoint(Point),
    #[error("starting position edits require a root snapshot with no children")]
    SetupRequiresRoot,
    #[error("cannot allocate another {kind} label")]
    MarkupLabelLimit { kind: String },
    #[error("node {0:?} is not in the live document")]
    MissingNode(NodeId),
    #[error("cannot delete or promote the root")]
    RootOperation,
    #[error("invalid fixed handicap {handicap} for board {size:?}")]
    InvalidHandicap { handicap: u8, size: BoardSize },
    #[error("unknown rules {0:?}")]
    UnknownRules(String),
    #[error("SGF game tree has an empty sequence")]
    EmptyTree,
    #[error("no command to undo")]
    NothingToUndo,
    #[error("no command to redo")]
    NothingToRedo,
    #[error("document node IDs are exhausted")]
    NodeLimit,
    #[error("document revision is exhausted")]
    RevisionLimit,
    #[error("invalid {property} value {value:?} at node {node:?}")]
    InvalidProperty {
        node: NodeId,
        property: String,
        value: String,
    },
    #[error("illegal move at node {node:?}: {reason}")]
    IllegalMove { node: NodeId, reason: IllegalMove },
    #[error("node {node:?} expects {expected:?} to play, got {actual:?}")]
    WrongPlayer {
        node: NodeId,
        expected: Color,
        actual: Color,
    },
    #[error("invalid setup at node {node:?}: {reason}")]
    InvalidSetup { node: NodeId, reason: SetupError },
    #[error(transparent)]
    InvalidBoardSize(#[from] BoardSizeError),
}

/// Raw properties stay attached to their original node, including unknown data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentNode {
    pub(crate) id: NodeId,
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<NodeId>,
    pub(crate) properties: Vec<Property>,
    pub(crate) live: bool,
}

impl DocumentNode {
    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    pub fn properties(&self) -> &[Property] {
        &self.properties
    }

    /// The first comment; repeated properties are still available in properties().
    pub fn comment(&self) -> Option<&str> {
        self.properties
            .iter()
            .find(|property| property.identifier == "C")
            .and_then(|property| property.values.first())
            .map(String::as_str)
    }
}

/// Logical content for rendering/comparison. Revisions, history and tombstones
/// are deliberately excluded: undo restores content, never the revision clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSnapshot {
    pub root: NodeId,
    pub current: NodeId,
    pub nodes: Vec<DocumentNode>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Captures {
    pub black: usize,
    pub white: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GamePosition {
    pub board: Position,
    /// Row-major, matching board.stones(). Empty and explicitly set up points
    /// have no number. Ordinary moves count from the root, including passes;
    /// newly played preview stones instead use their one-based PV index.
    pub stone_move_numbers: Vec<Option<usize>>,
    /// Source node for each surviving ordinary stone. Preview stones have no
    /// document node, even when their point matches a recorded continuation.
    pub stone_node_ids: Vec<Option<NodeId>>,
    /// All moves from the root, including passes, across setup boundaries.
    pub moves: Vec<Move>,
    /// Effective starting stones after the most recent setup or turn reset.
    pub initial_stones: Vec<(Color, Point)>,
    pub initial_player: Color,
    /// Replay these from initial_stones/initial_player to obtain board.
    pub analysis_moves: Vec<Move>,
    pub rules: RulePreset,
    pub rules_raw: Option<String>,
    pub komi: f64,
    pub handicap: u32,
    pub captured_by: Captures,
}
