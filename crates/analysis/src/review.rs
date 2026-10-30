//! Batched whole-game queries, owned and polled by the application actor.
//! No result storage or document mutation lives here. The caller retains each
//! complete response by node/key and builds a fresh plan from unfinished nodes
//! after a pause, so completed work is never submitted again.

use crate::{AnalysisError, LiveSettings, request};
use document::{AnalysisKey, GamePosition, NodeId};
use engine_session::{AnalysisRequest, EngineHandle, QueryEvent, QuerySubscription};
use katago_protocol::AnalysisResponse;
use std::collections::BTreeMap;

/// Lower than live analysis (0). The application reserves a concurrent search
/// slot for live analysis; priority alone cannot preempt an already running job.
pub const REVIEW_PRIORITY: i64 = -100;
const MAX_TURNS: usize = 256;

type NodesByTurn = BTreeMap<u32, Vec<(NodeId, AnalysisKey)>>;

/// A validated plan for the supplied line, including any setup/PL boundaries.
pub struct ReviewPlan {
    batches: Vec<ReviewBatch>,
}

/// A query and its exact node mapping. Multiple empty SGF nodes can share a turn.
pub struct ReviewBatch {
    pub request: AnalysisRequest,
    nodes: NodesByTurn,
    target_visits: u64,
}

/// The entire engine response, including root statistics, candidates and PVs.
/// All values retain the engine's configured Black perspective. `key` binds
/// this result to the analyzed position, independently of later navigation.
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewResult {
    pub node_id: NodeId,
    pub key: AnalysisKey,
    pub response: AnalysisResponse,
}

/// One pending batch. The owner must cancel it before discarding a live run.
pub struct ReviewRun {
    subscription: QuerySubscription,
    nodes: NodesByTurn,
    target_visits: u64,
    done: bool,
}

impl ReviewPlan {
    pub fn new(
        positions: Vec<(NodeId, GamePosition)>,
        target_visits: u64,
    ) -> Result<Self, AnalysisError> {
        if !(500..=1_000_000).contains(&target_visits) {
            return Err(AnalysisError::InvalidSettings(
                "whole-game visits must be between 500 and 1000000",
            ));
        }
        let settings = LiveSettings {
            max_visits: target_visits,
            ..LiveSettings::default()
        };
        let mut batches: Vec<ReviewBatch> = Vec::new();
        for (node_id, position) in positions {
            let mut query = request::build(&position, &settings)?;
            let turn = query.moves.len() as u32; // build validated the u32 bound.
            query.priority = Some(REVIEW_PRIORITY);
            let key = position.analysis_key();
            let compatible = batches.last().is_some_and(|batch| {
                same_replay(&batch.request, &query)
                    && (batch.nodes.contains_key(&turn) || batch.nodes.len() < MAX_TURNS)
            });
            if compatible {
                if let Some(batch) = batches.last_mut() {
                    batch.request.moves = query.moves;
                    batch.nodes.entry(turn).or_default().push((node_id, key));
                }
            } else {
                batches.push(ReviewBatch {
                    request: query,
                    nodes: BTreeMap::from([(turn, vec![(node_id, key)])]),
                    target_visits,
                });
            }
        }
        for batch in &mut batches {
            batch.request.analyze_turns = Some(batch.nodes.keys().copied().collect());
        }
        Ok(Self { batches })
    }

    pub fn into_batches(self) -> Vec<ReviewBatch> {
        self.batches
    }
}

fn same_replay(before: &AnalysisRequest, after: &AnalysisRequest) -> bool {
    before.board_x_size == after.board_x_size
        && before.board_y_size == after.board_y_size
        && before.rules == after.rules
        && before.komi == after.komi
        && before.initial_stones == after.initial_stones
        && before.initial_player == after.initial_player
        && after.moves.starts_with(&before.moves)
}

impl ReviewBatch {
    pub async fn submit(self, engine: &EngineHandle) -> Result<ReviewRun, AnalysisError> {
        let subscription = engine.submit(self.request).await?;
        Ok(ReviewRun {
            subscription,
            nodes: self.nodes,
            target_visits: self.target_visits,
            done: false,
        })
    }
}

impl ReviewRun {
    /// Stable subscription id, including after an engine restart.
    pub fn id(&self) -> &str {
        self.subscription.id()
    }

    /// Cancellation safe. Only final, sufficiently deep results complete nodes.
    /// engine-session replays unfinished turns after restart; this layer never
    /// submits a second copy. An unexpected end is an explicit error.
    pub async fn recv(&mut self) -> Option<Result<Vec<ReviewResult>, AnalysisError>> {
        if self.done {
            return None;
        }
        loop {
            match self.subscription.recv().await {
                Some(QueryEvent::Final(response)) => {
                    let Some(nodes) = self.nodes.remove(&response.turn_number) else {
                        self.done = true;
                        return Some(Err(AnalysisError::Review(
                            "engine returned an unexpected whole-game turn".into(),
                        )));
                    };
                    if response.root_info.visits < self.target_visits {
                        self.done = true;
                        return Some(Err(AnalysisError::Review(format!(
                            "whole-game turn {} ended at {} visits, below target {}",
                            response.turn_number, response.root_info.visits, self.target_visits,
                        ))));
                    }
                    self.done = self.nodes.is_empty();
                    return Some(Ok(nodes
                        .into_iter()
                        .map(|(node_id, key)| ReviewResult {
                            node_id,
                            key,
                            response: response.clone(),
                        })
                        .collect()));
                }
                Some(QueryEvent::Partial(_) | QueryEvent::Restarted) => {}
                Some(QueryEvent::Failed(error)) => {
                    self.done = true;
                    return Some(Err(error.into()));
                }
                Some(QueryEvent::Terminated) | None => {
                    self.done = true;
                    return Some(Err(AnalysisError::Review(
                        "whole-game query ended before all positions completed".into(),
                    )));
                }
            }
        }
    }

    /// Awaiting success acknowledges termination and discards unread results.
    pub async fn cancel(&mut self, engine: &EngineHandle) -> Result<(), AnalysisError> {
        engine.terminate(self.id().to_owned()).await?;
        self.done = true;
        self.nodes.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
