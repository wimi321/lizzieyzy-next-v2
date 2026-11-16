use super::{Actor, Client};
use crate::{
    WebAnalysisUpdate, WebFullState, WebHello, WebServerMessage as Message, WebTrialBranch,
    WebTrialState,
};

impl Actor {
    pub(in crate::actor) fn refresh_web_board(&mut self) {
        self.publish_web_status();
        if self.web_board.service.is_none() {
            return;
        }
        let stamp = self.output.web_stamp();
        if self.web_board.stamp == Some(stamp) {
            return;
        }
        self.web_board.stamp = Some(stamp);
        let Some((revision, document, published_analysis)) = self.output.web_snapshot() else {
            return;
        };
        let analysis = document
            .restricted_analysis
            .as_deref()
            .cloned()
            .or(published_analysis);
        let full = WebFullState {
            revision,
            node_id: document.tree.current,
            board: document.board.clone(),
            analysis,
        };
        let position_changed = self.web_board.snapshot.as_ref().is_none_or(|old| {
            old.revision != full.revision || old.node_id != full.node_id || old.board != full.board
        });
        if position_changed {
            self.web_broadcast(Message::FullState(full.clone()));
            self.web_hello_all();
        } else if self.web_board.snapshot.as_ref() != Some(&full) {
            self.web_broadcast(Message::AnalysisUpdate(WebAnalysisUpdate {
                revision,
                node_id: full.node_id,
                board: full.board.as_ref().map(Into::into),
                analysis: full.analysis.clone(),
            }));
        }
        let mut history = document.graph;
        history.points.truncate(history.current_index + 1);
        if let Some(analysis) = &full.analysis {
            for point in &mut history.points {
                if point.node_id == analysis.node_id {
                    point.summary = Some(analysis.summary);
                }
            }
        }
        if self.web_board.history.as_ref() != Some(&history) {
            self.web_broadcast(Message::WinrateHistory(history.clone()));
            self.web_board.history = Some(history);
        }
        if let Ok(problems) =
            self.problem_moves_with_settings(Default::default(), &Default::default())
            && self.web_board.problems.as_ref() != Some(&problems)
        {
            self.web_broadcast(Message::ProblemMoves(problems.clone()));
            self.web_board.problems = Some(problems);
        }
        let trial = self.web_trial_view();
        if self.web_board.trial_view.as_ref() != Some(&trial) {
            self.web_broadcast(Message::TrialState(trial.clone()));
            self.web_board.trial_view = Some(trial);
        }
        self.web_board.snapshot = Some(full);
    }

    fn web_hello(&self, client: &Client) -> Option<WebHello> {
        let info = self.document.game_info().ok()?;
        Some(WebHello {
            client_id: client.id.clone(),
            reconnect: client.reconnect.clone(),
            komi: info.komi,
            rules: info.rules_preset,
            allow_trial: self.settings.web_board.allow_trial,
        })
    }

    pub(super) fn web_hello_all(&self) {
        for client in self.web_board.clients.values() {
            if let Some(hello) = self.web_hello(client) {
                client.send(Message::Hello(hello));
            }
        }
    }

    pub(super) fn web_initial(&self, client: &Client) {
        if let Some(hello) = self.web_hello(client) {
            client.send(Message::Hello(hello));
        }
        if let Some(value) = &self.web_board.snapshot {
            client.send(Message::FullState(value.clone()));
        }
        if let Some(value) = &self.web_board.history {
            client.send(Message::WinrateHistory(value.clone()));
        }
        if let Some(value) = &self.web_board.problems {
            client.send(Message::ProblemMoves(value.clone()));
        }
        client.send(Message::TrialState(self.web_trial_view()));
    }

    fn web_trial_view(&self) -> WebTrialState {
        let Some(trial) = &self.web_board.trial else {
            return WebTrialState::default();
        };
        let current = self.document.current();
        let node = self.tree.nodes.iter().find(|node| node.id == current.0);
        let children = node
            .map(|node| node.children.as_slice())
            .unwrap_or_default();
        let branches = children
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let node = self.tree.nodes.iter().find(|node| node.id == *id)?;
                let (x, y, label) = match node.mv {
                    Some(crate::MoveView::Play { point, .. }) => {
                        (Some(point.x), Some(point.y), (index + 1).to_string())
                    }
                    Some(crate::MoveView::Pass { .. }) => (None, None, "pass".to_owned()),
                    None => (None, None, (index + 1).to_string()),
                };
                Some(WebTrialBranch { x, y, label, index })
            })
            .collect();
        WebTrialState {
            active: true,
            owner: Some(trial.owner.clone()),
            anchor_move: trial.anchor_move,
            move_number: node.map_or(trial.anchor_move, |node| node.move_number),
            can_back: current != trial.anchor,
            can_forward: !children.is_empty(),
            branches,
        }
    }
}
