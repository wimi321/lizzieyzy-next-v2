//! Manual scoring holds a frozen position and never edits stones or analysis.

use super::Actor;
use crate::{
    AppError, GamePhase, PointView, ScoringError, ScoringRuleView, ScoringView, scoring::Request,
};
use document::GamePosition;
use go_core::{
    Color, Point,
    scoring::{ScoreOptions, stone_group},
};
use std::collections::HashSet;

/// The group's average must favor the opponent by at least 0.8. Values already
/// use Black's perspective; White-to-play does not change their sign.
const DEAD_OWNERSHIP_THRESHOLD: f64 = 0.8;

pub(super) struct Session {
    node_id: u32,
    position: GamePosition,
    rule: ScoringRuleView,
    dead: HashSet<Point>,
    neutral: HashSet<Point>,
}

impl Session {
    fn view(&self) -> Result<ScoringView, AppError> {
        let score = go_core::scoring::score(
            &self.position.board,
            &self.dead,
            &self.neutral,
            ScoreOptions {
                rule: self.rule.into(),
                preset: self.position.rules,
                komi: self.position.komi,
                handicap: self.position.handicap,
                captured_by: [
                    self.position.captured_by.black,
                    self.position.captured_by.white,
                ],
            },
        )
        .map_err(|_| ScoringError::InvalidPosition)?;
        Ok(ScoringView {
            node_id: self.node_id,
            rule: self.rule,
            dead_stones: sorted_points(&self.dead),
            neutral_points: sorted_points(&self.neutral),
            ownership: score
                .ownership
                .into_iter()
                .map(|owner| owner.map(Into::into))
                .collect(),
            black: score.black.into(),
            white: score.white.into(),
            score_lead: score.score_lead,
            result: score.result,
        })
    }
}

impl Actor {
    pub(super) async fn scoring_command(&mut self, request: Request) {
        match request {
            Request::Start(reply) => {
                let result = self.start_scoring().await;
                let _ = reply.send(result);
            }
            Request::Toggle(point, reply) => {
                let result = self.toggle_scoring_point(point);
                let _ = reply.send(result);
            }
            Request::Rule(rule, reply) => {
                let result = self.set_scoring_rule(rule);
                let _ = reply.send(result);
            }
            Request::Commit(reply) => {
                let result = self.commit_scoring().await;
                let _ = reply.send(result);
            }
            Request::Cancel(reply) => {
                let result = self.cancel_scoring().await;
                let _ = reply.send(result);
            }
        }
    }

    pub(super) fn scoring_view(&self) -> Result<Option<ScoringView>, AppError> {
        self.scoring.as_ref().map(Session::view).transpose()
    }

    /// Restore the current user intent after a successful document mutation.
    /// An unchanged pondering value preserves the coordinator's limit reason.
    pub(super) async fn exit_scoring_for_document_change(&mut self) -> Result<(), AppError> {
        if self.scoring.take().is_some() {
            self.update_coordinators(super::coordinators::Update::Pondering(self.pondering()))
                .await?;
        }
        Ok(())
    }

    async fn start_scoring(&mut self) -> Result<ScoringView, AppError> {
        if self
            .game
            .as_ref()
            .is_some_and(|game| game.view.phase != GamePhase::Finished)
            || self.match_active()
            || self.coach.is_starting()
            || self.coach.session_active()
        {
            return Err(ScoringError::GameActive.into());
        }
        self.ensure_research_available()?;
        if let Some(session) = &self.scoring {
            return session.view();
        }
        let node_id = self.document.current().0;
        let position = self.document.position_at(self.document.current())?;
        let ownership = self
            .stored_analysis(node_id)
            .and_then(|stored| stored.response.ownership);
        let dead = initial_dead(&position, ownership.as_deref());
        let session = Session {
            node_id,
            rule: position.rules.rules().scoring.into(),
            position,
            dead,
            neutral: HashSet::new(),
        };
        let view = session.view()?;
        self.update_coordinators(super::coordinators::Update::Clear)
            .await?;
        self.flush_cached_focus().await;
        self.focus = None;
        self.output.clear_analysis();
        self.scoring = Some(session);
        self.publish_document()?;
        self.publish_analysis_state();
        Ok(view)
    }

    fn toggle_scoring_point(&mut self, point: Point) -> Result<ScoringView, AppError> {
        let session = self.scoring.as_mut().ok_or(ScoringError::NotStarted)?;
        if !session.position.board.size().contains(point) {
            return Err(ScoringError::OutOfBounds.into());
        }
        if session.position.board.stone_at(point).is_some() {
            let dead = !session.dead.contains(&point);
            for member in stone_group(&session.position.board, point) {
                if dead {
                    session.dead.insert(member);
                } else {
                    session.dead.remove(&member);
                }
            }
        } else if !session.neutral.remove(&point) {
            session.neutral.insert(point);
        }
        let view = session.view()?;
        self.publish_document()?;
        Ok(view)
    }

    fn set_scoring_rule(&mut self, rule: ScoringRuleView) -> Result<ScoringView, AppError> {
        let session = self.scoring.as_mut().ok_or(ScoringError::NotStarted)?;
        session.rule = rule;
        let view = session.view()?;
        self.publish_document()?;
        Ok(view)
    }

    async fn commit_scoring(&mut self) -> Result<u64, AppError> {
        let view = self
            .scoring
            .as_ref()
            .ok_or(ScoringError::NotStarted)?
            .view()?;
        self.ensure_research_available()?;
        self.next_revision()?;
        let before = self.document.revision();
        self.set_game_result(Some(view.result))?;
        if self.document.revision() != before {
            // The regular document-change hook exits scoring and restores focus.
            self.document_changed().await?;
        } else {
            self.cancel_scoring().await?;
        }
        self.publish_analysis_state();
        Ok(self.revision)
    }

    async fn cancel_scoring(&mut self) -> Result<(), AppError> {
        if self.scoring.is_none() {
            return Ok(());
        }
        self.exit_scoring_for_document_change().await?;
        self.publish_document()?;
        self.update_focus().await;
        self.publish_analysis_state();
        Ok(())
    }
}

fn sorted_points(points: &HashSet<Point>) -> Vec<PointView> {
    let mut points: Vec<_> = points
        .iter()
        .map(|p| PointView { x: p.x, y: p.y })
        .collect();
    points.sort_by_key(|p| (p.y, p.x));
    points
}

fn initial_dead(position: &GamePosition, ownership: Option<&[f64]>) -> HashSet<Point> {
    let size = position.board.size();
    let Some(ownership) = ownership.filter(|values| values.len() == size.area()) else {
        return HashSet::new();
    };
    let mut visited = HashSet::new();
    let mut dead = HashSet::new();
    for (index, stone) in position.board.stones().iter().enumerate() {
        let Some(color) = stone else {
            continue;
        };
        let point = Point {
            x: (index % usize::from(size.width)) as u8,
            y: (index / usize::from(size.width)) as u8,
        };
        if visited.contains(&point) {
            continue;
        }
        let group = stone_group(&position.board, point);
        visited.extend(group.iter().copied());
        let values: Vec<_> = group
            .iter()
            .map(|p| ownership[usize::from(p.y) * usize::from(size.width) + usize::from(p.x)])
            .collect();
        if values
            .iter()
            .any(|v| !v.is_finite() || !(-1.0..=1.0).contains(v))
        {
            continue;
        }
        let average = values.iter().sum::<f64>() / values.len() as f64;
        let is_dead = match color {
            Color::Black => average <= -DEAD_OWNERSHIP_THRESHOLD,
            Color::White => average >= DEAD_OWNERSHIP_THRESHOLD,
        };
        if is_dead {
            dead.extend(group);
        }
    }
    dead
}

#[cfg(test)]
mod tests;
