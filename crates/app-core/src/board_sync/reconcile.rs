//! Conservative snapshot reconciliation, following Java's SyncSnapshot* policies.
//!
//! A snapshot cannot reconstruct unseen moves. Only a verified single move is
//! appended; discontinuities become SGF setup positions after conflict confirmation.

use document::{Command, DocError, GameDocument, GamePosition, NodeId};
use go_core::{Color, Move, Point};
use readboard::{RemoteContext, Snapshot};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReconcileOutcome {
    pub changed: bool,
    pub rebuilt: bool,
    pub held: bool,
    /// The remote position to analyze, even while the user browses another node.
    pub resolved_node: NodeId,
}

#[derive(Debug, Default)]
pub(crate) struct Reconciler {
    pending_conflict: Option<Snapshot>,
    last_resolved: Option<NodeId>,
    context: Option<RemoteContext>,
    /// Remote numbering is independent of the known moves after a setup root.
    remote_numbers: BTreeMap<NodeId, u32>,
    markers: BTreeMap<NodeId, (Color, Point)>,
}

impl Reconciler {
    pub fn apply(
        &mut self,
        document: &mut GameDocument,
        snapshot: &Snapshot,
        always_keep_consistent: bool,
        always_goto_last: bool,
    ) -> Result<ReconcileOutcome, DocError> {
        validate(snapshot, document.root())?;
        let end = main_end(document)?;
        let start = document.position_at(end).ok();
        let viewed = document.current();
        let old_revision = document.revision();
        let context_changed = self
            .context
            .as_ref()
            .is_some_and(|previous| conflicts(previous, &snapshot.context));
        if !snapshot.force_rebuild
            && !context_changed
            && start
                .as_ref()
                .is_some_and(|p| p.board.size() == snapshot.size)
        {
            // Fox live-room titles can trail an already accepted visual move by
            // one frame. Keep its proven history instead of flattening it.
            if self.last_resolved == Some(end)
                && viewed == end
                && snapshot.context.live_title_move.is_some()
                && recovery_number(snapshot).is_some_and(|number| {
                    number.checked_add(1) == self.number_at(end, start.as_ref())
                })
                && start.as_ref().is_some_and(|position| {
                    !position.moves.is_empty() && position.board.stones() == snapshot.stones()
                })
            {
                self.pending_conflict = None;
                return Ok(ReconcileOutcome {
                    changed: false,
                    rebuilt: false,
                    held: false,
                    resolved_node: end,
                });
            }
            if let Some(matched) = self.match_history(document, end, snapshot)? {
                // A repeat must not undo a deliberate local navigation. A remote
                // history jump is followed only if the view was following it.
                if self.last_resolved != Some(matched)
                    && (self.last_resolved.is_none()
                        || self.last_resolved == Some(viewed)
                        || always_goto_last)
                {
                    document.apply(Command::GoTo(matched))?;
                }
                self.accept(matched, snapshot);
                return Ok(ReconcileOutcome {
                    changed: document.revision() != old_revision,
                    rebuilt: false,
                    held: false,
                    resolved_node: matched,
                });
            }
            if let Some(position) = &start
                && let Some(mv) = self.single_move(end, position, snapshot)
            {
                // All speculative edits happen on a clone; a failed move never
                // changes the user's document or its undo history.
                let mut candidate = document.clone();
                candidate.apply(Command::GoTo(end))?;
                if candidate.apply(Command::Play(mv)).is_ok() {
                    let resolved = candidate.current();
                    if always_keep_consistent {
                        // Java advances to the synchronized main end here. View
                        // restoration below remains a separate option, exactly
                        // as restoreViewedNodeAfterSync does in the reference.
                        candidate.apply(Command::GoTo(main_end(&candidate)?))?;
                    }
                    if !always_goto_last && viewed != end {
                        candidate.apply(Command::GoTo(viewed))?;
                    }
                    *document = candidate;
                    self.accept(resolved, snapshot);
                    return Ok(ReconcileOutcome {
                        changed: true,
                        rebuilt: false,
                        held: false,
                        resolved_node: resolved,
                    });
                }
            }
        }

        let has_history = document.node(end)?.parent().is_some();
        let number_changed = recovery_number(snapshot)
            .is_some_and(|number| self.number_at(end, start.as_ref()) != Some(number));
        let immediate = snapshot.force_rebuild
            || context_changed
            || !has_history
            || start
                .as_ref()
                .is_none_or(|p| p.board.size() != snapshot.size)
            || (recovery_number(snapshot).is_some() && self.last_resolved.is_none())
            || number_changed;
        if !immediate
            && !self
                .pending_conflict
                .as_ref()
                .is_some_and(|previous| same_conflict(previous, snapshot))
        {
            self.pending_conflict = Some(snapshot.clone());
            return Ok(ReconcileOutcome {
                changed: false,
                rebuilt: false,
                held: true,
                resolved_node: self.last_resolved.unwrap_or(end),
            });
        }
        let rebuilt = rebuild(document, snapshot, start.as_ref())?;
        let resolved = rebuilt.current();
        *document = rebuilt;
        self.remote_numbers.clear();
        self.markers.clear();
        self.accept(resolved, snapshot);
        Ok(ReconcileOutcome {
            changed: true,
            rebuilt: true,
            held: false,
            resolved_node: resolved,
        })
    }

    fn accept(&mut self, node: NodeId, snapshot: &Snapshot) {
        self.pending_conflict = None;
        self.last_resolved = Some(node);
        self.context = Some(snapshot.context.clone());
        if let Some(number) = recovery_number(snapshot) {
            self.remote_numbers.insert(node, number);
        }
        if let Some(marker) = snapshot.last_move() {
            self.markers.insert(node, marker);
        }
    }

    fn number_at(&self, node: NodeId, position: Option<&GamePosition>) -> Option<u32> {
        self.remote_numbers
            .get(&node)
            .copied()
            .or_else(|| position.and_then(|p| u32::try_from(p.moves.len()).ok()))
    }

    fn match_history(
        &self,
        document: &GameDocument,
        end: NodeId,
        snapshot: &Snapshot,
    ) -> Result<Option<NodeId>, DocError> {
        let number = recovery_number(snapshot);
        let mut cursor = Some(end);
        while let Some(node) = cursor {
            if let Ok(position) = document.position_at(node) {
                let stones_match = if number.is_some() {
                    // Java tolerates Fox marker color jitter only on an occupied
                    // point; the rest of the layout and the move number must match.
                    position.board.size() == snapshot.size
                        && position.board.stones().iter().zip(&snapshot.cells).all(
                            |(stone, cell)| {
                                if cell.is_last_move() {
                                    stone.is_some()
                                } else {
                                    *stone == cell.color()
                                }
                            },
                        )
                } else {
                    position.board.size() == snapshot.size
                        && position.board.stones() == snapshot.stones()
                };
                let identity_match = match number {
                    Some(number) => self.number_at(node, Some(&position)) == Some(number),
                    None => snapshot.last_move().is_none_or(|marker| {
                        position.moves.last() == Some(&Move::Play(marker.0, marker.1))
                            || self.markers.get(&node) == Some(&marker)
                    }),
                };
                if stones_match && identity_match {
                    return Ok(Some(node));
                }
            }
            // Generic captures lack trustworthy history identity.
            cursor = if number.is_some() {
                document.node(node)?.parent()
            } else {
                None
            };
        }
        Ok(None)
    }

    fn single_move(&self, end: NodeId, start: &GamePosition, snapshot: &Snapshot) -> Option<Move> {
        let number = recovery_number(snapshot);
        if let Some(number) = number
            && self.number_at(end, Some(start))?.checked_add(1) != Some(number)
        {
            return None;
        }
        let mut additions = start
            .board
            .stones()
            .iter()
            .zip(&snapshot.cells)
            .enumerate()
            .filter(|(_, (old, new))| old.is_none() && new.color().is_some());
        let (index, (_, cell)) = additions.next()?;
        if additions.next().is_some() {
            return None;
        }
        let color = cell.color()?;
        let point = Point {
            x: (index % usize::from(snapshot.size.width)) as u8,
            y: (index / usize::from(snapshot.size.width)) as u8,
        };
        if let Some(marker) = snapshot.last_move() {
            if marker != (color, point) {
                return None;
            }
        } else if number.is_none()
            || start
                .board
                .stones()
                .iter()
                .zip(&snapshot.cells)
                .any(|(old, new)| old.is_some() && new.color().is_none())
        {
            // Java's markerless Fox shortcut is only for one addition, never a
            // capture; captures require a matching last-move marker.
            return None;
        }
        if color != start.board.to_play() {
            return None;
        }
        let mut board = start.board.clone();
        let mv = Move::Play(color, point);
        board.play(mv).ok()?;
        (board.stones() == snapshot.stones()).then_some(mv)
    }
}

fn validate(snapshot: &Snapshot, root: NodeId) -> Result<(), DocError> {
    snapshot.size.validate()?;
    if snapshot.cells.len() != snapshot.size.area()
        || snapshot
            .cells
            .iter()
            .filter(|cell| cell.is_last_move())
            .count()
            > 1
    {
        return Err(DocError::InvalidProperty {
            node: root,
            property: "readboard".to_owned(),
            value: "incomplete snapshot or multiple last-move markers".to_owned(),
        });
    }
    Ok(())
}

fn main_end(document: &GameDocument) -> Result<NodeId, DocError> {
    let mut node = document.root();
    while let Some(child) = document.node(node)?.children().first() {
        node = *child;
    }
    Ok(node)
}

fn recovery_number(snapshot: &Snapshot) -> Option<u32> {
    if !snapshot
        .context
        .platform
        .as_deref()
        .is_some_and(|p| p.eq_ignore_ascii_case("fox"))
    {
        return None;
    }
    let number = snapshot.fox_move_number?;
    let title_number = snapshot
        .context
        .record_current_move
        .or_else(|| {
            (snapshot.context.record_at_end == Some(true))
                .then_some(snapshot.context.record_total_move)
                .flatten()
        })
        .or(snapshot.context.live_title_move);
    if title_number.is_some_and(|title| title != number) {
        return None;
    }
    Some(number)
}

fn conflicts(previous: &RemoteContext, current: &RemoteContext) -> bool {
    fn changed<T: PartialEq>(left: &Option<T>, right: &Option<T>) -> bool {
        matches!((left, right), (Some(left), Some(right)) if left != right)
    }
    changed(&previous.platform, &current.platform)
        || changed(&previous.room_token, &current.room_token)
        || changed(
            &previous.record_title_fingerprint,
            &current.record_title_fingerprint,
        )
        || changed(&previous.record_total_move, &current.record_total_move)
}

fn same_conflict(left: &Snapshot, right: &Snapshot) -> bool {
    left.size == right.size
        && left.stones() == right.stones()
        && recovery_number(left) == recovery_number(right)
        && !conflicts(&left.context, &right.context)
}

fn rebuild(
    document: &GameDocument,
    snapshot: &Snapshot,
    previous: Option<&GamePosition>,
) -> Result<GameDocument, DocError> {
    // Retain game information, never position markup or stale analysis payloads.
    let mut properties: Vec<_> = document
        .node(document.root())?
        .properties()
        .iter()
        .filter(|p| {
            matches!(
                p.identifier.as_str(),
                "GM" | "FF"
                    | "CA"
                    | "AP"
                    | "ST"
                    | "PB"
                    | "BR"
                    | "PW"
                    | "WR"
                    | "BT"
                    | "WT"
                    | "GN"
                    | "GC"
                    | "EV"
                    | "RO"
                    | "DT"
                    | "PC"
                    | "RU"
                    | "KM"
                    | "KO"
                    | "HA"
                    | "RE"
                    | "OT"
                    | "TM"
                    | "SO"
                    | "US"
                    | "AN"
                    | "CP"
            )
        })
        .cloned()
        .collect();
    let size = if snapshot.size.width == snapshot.size.height {
        snapshot.size.width.to_string()
    } else {
        format!("{}:{}", snapshot.size.width, snapshot.size.height)
    };
    properties.push(property("SZ", vec![size]));
    for (color, name) in [(Color::Black, "AB"), (Color::White, "AW")] {
        let values: Vec<_> = snapshot
            .cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.color() == Some(color))
            .map(|(index, _)| {
                let x = (index % usize::from(snapshot.size.width)) as u8;
                let y = (index / usize::from(snapshot.size.width)) as u8;
                String::from_iter([char::from(b'a' + x), char::from(b'a' + y)])
            })
            .collect();
        if !values.is_empty() {
            properties.push(property(name, values));
        }
    }
    let to_play = inferred_player(snapshot, previous);
    properties.push(property(
        "PL",
        vec![
            match to_play {
                Color::Black => "B",
                Color::White => "W",
            }
            .to_owned(),
        ],
    ));
    if let Some(number) = recovery_number(snapshot) {
        // MN preserves the remote display number without inventing preceding plays.
        properties.push(property("MN", vec![number.to_string()]));
    }
    let rebuilt = GameDocument::from_sgf(&sgf::GameTree {
        sequence: vec![sgf::Node { properties }],
        children: Vec::new(),
    })?;
    // from_sgf deliberately retains illegal SGF. Recognition must not replace a
    // valid document with zero-liberty groups or another invalid setup.
    rebuilt.position_at(rebuilt.root())?;
    Ok(rebuilt)
}

fn inferred_player(snapshot: &Snapshot, previous: Option<&GamePosition>) -> Color {
    let stones = snapshot.stones();
    if stones.iter().all(Option::is_none) {
        return Color::Black;
    }
    if recovery_number(snapshot) == Some(0)
        && snapshot.last_move().is_none()
        && stones.iter().filter(|c| **c == Some(Color::Black)).count() >= 2
        && !stones.contains(&Some(Color::White))
    {
        return Color::White;
    }
    if matches!(
        snapshot.context.last_move_source.as_deref(),
        Some("redBlueMarker" | "foxCornerFlip")
    ) && let Some((color, _)) = snapshot.last_move()
    {
        return color.opponent();
    }
    // An unmarked frame cannot prove alternating turns (handicap and setup are
    // valid); Java preserves the prior turn unless it has a trusted visual mark.
    previous.map_or(Color::Black, |position| position.board.to_play())
}

fn property(name: &str, values: Vec<String>) -> sgf::Property {
    sgf::Property {
        identifier: name.to_owned(),
        values,
    }
}

#[cfg(test)]
mod tests;
