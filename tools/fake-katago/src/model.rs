//! Deliberately small deterministic simulator, not a Go evaluator.
use std::collections::BTreeMap;

use crate::cli::ReportPerspective;

use serde_json::{Value, json};
use thiserror::Error;

const LETTERS: &[u8] = b"ABCDEFGHJKLMNOPQRSTUVWXYZ";
pub const FIELDS: &[&str] = &[
    "id",
    "moves",
    "initialStones",
    "initialPlayer",
    "rules",
    "komi",
    "boardXSize",
    "boardYSize",
    "analyzeTurns",
    "maxVisits",
    "includeOwnership",
    "includePolicy",
    "reportDuringSearchEvery",
    "firstReportDuringSearchAfter",
    "priority",
    "priorities",
    "overrideSettings",
];

#[derive(Debug, Error)]
#[error("{message}")]
pub struct QueryError {
    pub field: &'static str,
    pub message: String,
}

fn invalid(field: &'static str) -> QueryError {
    QueryError {
        field,
        message: format!("Invalid {field}"),
    }
}

#[derive(Clone)]
pub struct Query {
    pub id: String,
    pub width: usize,
    pub height: usize,
    pub moves: Vec<(u8, String)>,
    pub initial: Vec<(u8, String)>,
    pub initial_player: u8,
    pub turns: Vec<usize>,
    pub priorities: Vec<i64>,
    pub max_visits: u64,
    pub interval: Option<f64>,
    pub first_report: f64,
    pub ownership: bool,
    pub policy: bool,
    allow_suicide: bool,
}

impl Query {
    pub fn parse(v: &Value) -> Result<Self, QueryError> {
        let id = v["id"].as_str().ok_or_else(|| invalid("id"))?.to_owned();
        let width = size(v, "boardXSize")?;
        let height = size(v, "boardYSize")?;
        let rules = &v["rules"];
        if !rules.is_string() && !rules.is_object() {
            return Err(invalid("rules"));
        }
        if v.get("komi").is_some() && v["komi"].as_f64().is_none() {
            return Err(invalid("komi"));
        }
        let moves = parse_moves(v.get("moves"), "moves", true)?;
        let initial = parse_moves(v.get("initialStones"), "initialStones", false)?;
        let initial_player = match v.get("initialPlayer") {
            Some(value) => color(value).ok_or_else(|| invalid("initialPlayer"))?,
            None => moves.first().map(|m| m.0).unwrap_or_else(|| {
                if initial.len() > 1 && initial.iter().all(|stone| stone.0 == 1) {
                    2
                } else {
                    1
                }
            }),
        };
        let turns = match v.get("analyzeTurns") {
            None => vec![moves.len()],
            Some(value) => value
                .as_array()
                .ok_or_else(|| invalid("analyzeTurns"))?
                .iter()
                .map(|n| {
                    n.as_u64()
                        .filter(|n| *n <= moves.len() as u64)
                        .map(|n| n as usize)
                        .ok_or_else(|| invalid("analyzeTurns"))
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        if v.get("priority").is_some() && v.get("priorities").is_some() {
            return Err(QueryError {
                field: "priority",
                message: "Cannot specify both priority and priorities".into(),
            });
        }
        let priority = integer(v, "priority", 0)?;
        let priorities = match v.get("priorities") {
            None => vec![priority; turns.len()],
            Some(value) => {
                let values = value.as_array().ok_or_else(|| invalid("priorities"))?;
                if values.len() != turns.len() || v.get("analyzeTurns").is_none() {
                    return Err(invalid("priorities"));
                }
                values
                    .iter()
                    .map(|n| n.as_i64().ok_or_else(|| invalid("priorities")))
                    .collect::<Result<Vec<_>, _>>()?
            }
        };
        // KataGo enqueues each requested turn once, in ascending turn order.
        // In a duplicate turn entry the last per-turn priority wins.
        let (turns, priorities) = turns
            .into_iter()
            .zip(priorities)
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .unzip();
        let max_visits = match v.get("maxVisits") {
            None => 100,
            Some(n) => n
                .as_u64()
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("maxVisits"))?,
        };
        let interval = number(v, "reportDuringSearchEvery", false)?;
        let first_report = number(v, "firstReportDuringSearchAfter", true)?;
        let interval = interval.or_else(|| first_report.map(|_| 1e30));
        let first_report = first_report.unwrap_or(interval.unwrap_or(0.0));
        let ownership = boolean(v, "includeOwnership")?;
        let policy = boolean(v, "includePolicy")?;
        if v.get("overrideSettings").is_some_and(|x| !x.is_object()) {
            return Err(invalid("overrideSettings"));
        }
        let allow_suicide = rules
            .as_str()
            .is_some_and(|r| matches!(r, "new-zealand" | "tromp-taylor"))
            || rules.get("multiStoneSuicideLegal").and_then(Value::as_bool) == Some(true);
        let query = Self {
            id,
            width,
            height,
            moves,
            initial,
            initial_player,
            turns,
            priorities,
            max_visits,
            interval,
            first_report,
            ownership,
            policy,
            allow_suicide,
        };
        query.board_at(query.moves.len())?;
        Ok(query)
    }

    pub fn board_at(&self, turn: usize) -> Result<Board, QueryError> {
        let mut board = Board {
            cells: vec![0; self.width * self.height],
            width: self.width,
            height: self.height,
        };
        for (player, point) in &self.initial {
            let index = board
                .index(point)
                .flatten()
                .ok_or_else(|| invalid("initialStones"))?;
            if board.cells[index] != 0 {
                return Err(invalid("initialStones"));
            }
            board.cells[index] = *player;
        }
        let mut previous = None;
        for (i, (player, point)) in self.moves.iter().take(turn).enumerate() {
            let before = board.cells.clone();
            let index = board.index(point).ok_or_else(|| invalid("moves"))?;
            if let Some(index) = index
                && (!board.play(*player, index, self.allow_suicide)
                    || previous.as_ref() == Some(&board.cells))
            {
                return Err(QueryError {
                    field: "moves",
                    message: format!("Illegal move {i}: {point}"),
                });
            }
            previous = Some(before);
        }
        Ok(board)
    }

    pub fn response(
        &self,
        turn: usize,
        visits: u64,
        partial: bool,
        perspective: ReportPerspective,
    ) -> Value {
        // Queries are validated before scheduling; the fallback is defensive only.
        let board = self.board_at(turn).unwrap_or_else(|_| Board {
            cells: vec![0; self.width * self.height],
            width: self.width,
            height: self.height,
        });
        let mut hash = 0xcbf29ce484222325_u64;
        let mut feed = |byte: u8| {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        };
        feed(self.width as u8);
        feed(self.height as u8);
        for (player, point) in self.initial.iter().chain(self.moves.iter().take(turn)) {
            feed(*player);
            for byte in point.bytes() {
                feed(byte);
            }
            feed(0xff);
        }
        for byte in (turn as u64).to_le_bytes() {
            feed(byte);
        }
        let player = self
            .moves
            .get(turn.wrapping_sub(1))
            .map(|m| 3 - m.0)
            .unwrap_or(self.initial_player);
        let previous = turn.checked_sub(1).and_then(|t| self.board_at(t).ok());
        let legal: Vec<bool> = board
            .cells
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let mut next = board.clone();
                c == 0
                    && next.play(player, i, self.allow_suicide)
                    && previous.as_ref().is_none_or(|p| p.cells != next.cells)
            })
            .collect();
        let mut candidates: Vec<_> = legal
            .iter()
            .enumerate()
            .filter_map(|(i, &allowed)| allowed.then_some(i))
            .collect();
        candidates.sort_by_key(|i| mix(hash.wrapping_add(*i as u64)));
        let chosen: Vec<String> = if candidates.is_empty() {
            vec!["pass".into()]
        } else {
            candidates.iter().take(3).map(|i| board.point(*i)).collect()
        };
        let winrate = 0.2 + (hash % 6000) as f64 / 10000.0;
        let score = (winrate - 0.5) * 30.0;
        let utility = winrate * 2.0 - 1.0;
        let count = chosen.len() as u64;
        let infos: Vec<_> = chosen
            .iter()
            .enumerate()
            .map(|(order, mv)| {
                let child_visits = visits / count + u64::from((order as u64) < visits % count);
                json!({
                    "move": mv, "visits": child_visits, "edgeVisits": child_visits,
                    "winrate": winrate, "scoreMean": score, "scoreLead": score,
                    "scoreSelfplay": score, "scoreStdev": 12.0, "utility": utility,
                    "prior": 1.0 / count as f64, "lcb": winrate - 0.05,
                    "utilityLcb": utility - 0.1, "weight": child_visits as f64,
                    "edgeWeight": child_visits as f64, "order": order,
                    "playSelectionValue": child_visits as f64, "pv": [mv]
                })
            })
            .collect();
        let mut result = json!({
            "id": self.id, "turnNumber": turn, "isDuringSearch": partial, "moveInfos": infos,
            "rootInfo": {
                "currentPlayer": if player == 1 { "B" } else { "W" },
                "visits": visits, "winrate": winrate, "scoreLead": score,
                "scoreSelfplay": score, "scoreStdev": 12.0, "utility": utility,
                "weight": visits as f64, "thisHash": format!("{hash:016X}{:016X}", mix(hash)),
                "symHash": format!("{hash:016X}{:016X}", mix(hash)),
                "rawWinrate": winrate, "rawLead": score, "rawScoreSelfplay": score,
                "rawScoreSelfplayStdev": 12.0, "rawNoResultProb": 0.0,
                "rawStWrError": 0.01, "rawStScoreError": 0.5, "rawVarTimeLeft": 100.0
            }
        });
        if self.ownership {
            result["ownership"] = json!(
                board
                    .cells
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        match c {
                            1 => 1.0,
                            2 => -1.0,
                            _ => (mix(hash.wrapping_add(i as u64)) % 201) as f64 / 100.0 - 1.0,
                        }
                    })
                    .collect::<Vec<_>>()
            );
        }
        if self.policy {
            let empty = legal.iter().filter(|&&allowed| allowed).count();
            let probability = 1.0 / (empty + 1) as f64;
            let mut values: Vec<_> = legal
                .iter()
                .map(|allowed| if *allowed { probability } else { -1.0 })
                .collect();
            values.push(probability);
            result["policy"] = json!(values);
        }
        let white = match perspective {
            ReportPerspective::Black => false,
            ReportPerspective::White => true,
            ReportPerspective::ToPlay => player == 2,
        };
        if white {
            invert_info(&mut result["rootInfo"]);
            if let Some(moves) = result["moveInfos"].as_array_mut() {
                for info in moves {
                    invert_info(info);
                }
            }
            if let Some(ownership) = result["ownership"].as_array_mut() {
                for value in ownership {
                    if let Some(number) = value.as_f64() {
                        *value = json!(-number);
                    }
                }
            }
        }
        result
    }
}

fn invert_info(info: &mut Value) {
    for field in ["winrate", "rawWinrate", "lcb"] {
        if let Some(value) = info[field].as_f64() {
            info[field] = json!(1.0 - value);
        }
    }
    for field in [
        "scoreMean",
        "scoreLead",
        "scoreSelfplay",
        "utility",
        "utilityLcb",
        "rawLead",
        "rawScoreSelfplay",
    ] {
        if let Some(value) = info[field].as_f64() {
            info[field] = json!(-value);
        }
    }
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn size(v: &Value, field: &'static str) -> Result<usize, QueryError> {
    v[field]
        .as_u64()
        .filter(|n| (2..=25).contains(n))
        .map(|n| n as usize)
        .ok_or_else(|| invalid(field))
}

fn color(v: &Value) -> Option<u8> {
    match v.as_str()?.to_ascii_lowercase().as_str() {
        "b" | "black" => Some(1),
        "w" | "white" => Some(2),
        _ => None,
    }
}

fn parse_moves(
    v: Option<&Value>,
    field: &'static str,
    required: bool,
) -> Result<Vec<(u8, String)>, QueryError> {
    let Some(v) = v else {
        return if required {
            Err(invalid(field))
        } else {
            Ok(Vec::new())
        };
    };
    v.as_array()
        .ok_or_else(|| invalid(field))?
        .iter()
        .map(|mv| {
            let pair = mv
                .as_array()
                .filter(|v| v.len() == 2)
                .ok_or_else(|| invalid(field))?;
            Ok((
                color(&pair[0]).ok_or_else(|| invalid(field))?,
                pair[1].as_str().ok_or_else(|| invalid(field))?.to_owned(),
            ))
        })
        .collect()
}

fn integer(v: &Value, field: &'static str, default: i64) -> Result<i64, QueryError> {
    match v.get(field) {
        Some(n) => n.as_i64().ok_or_else(|| invalid(field)),
        None => Ok(default),
    }
}

fn number(v: &Value, field: &'static str, zero: bool) -> Result<Option<f64>, QueryError> {
    match v.get(field) {
        Some(n) => n
            .as_f64()
            .filter(|n| n.is_finite() && (if zero { *n >= 0.0 } else { *n > 0.0 }))
            .map(Some)
            .ok_or_else(|| invalid(field)),
        None => Ok(None),
    }
}

fn boolean(v: &Value, field: &'static str) -> Result<bool, QueryError> {
    match v.get(field) {
        Some(n) => n.as_bool().ok_or_else(|| invalid(field)),
        None => Ok(false),
    }
}

#[derive(Clone)]
pub struct Board {
    cells: Vec<u8>,
    width: usize,
    height: usize,
}

impl Board {
    fn index(&self, point: &str) -> Option<Option<usize>> {
        if point.eq_ignore_ascii_case("pass") {
            return Some(None);
        }
        let bytes = point.as_bytes();
        let (&column, row) = bytes.split_first()?;
        let x = LETTERS
            .iter()
            .position(|c| *c == column.to_ascii_uppercase())?;
        let y = std::str::from_utf8(row).ok()?.parse::<usize>().ok()?;
        (x < self.width && (1..=self.height).contains(&y))
            .then(|| Some((self.height - y) * self.width + x))
    }

    fn point(&self, index: usize) -> String {
        format!(
            "{}{}",
            LETTERS[index % self.width] as char,
            self.height - index / self.width
        )
    }

    fn neighbors(&self, index: usize) -> impl Iterator<Item = usize> {
        let x = index % self.width;
        let y = index / self.width;
        [
            x.checked_sub(1).map(|x| y * self.width + x),
            (x + 1 < self.width).then_some(index + 1),
            y.checked_sub(1).map(|y| y * self.width + x),
            (y + 1 < self.height).then_some(index + self.width),
        ]
        .into_iter()
        .flatten()
    }

    fn group(&self, index: usize) -> (Vec<usize>, bool) {
        let mut seen = vec![false; self.cells.len()];
        let mut stones = vec![index];
        seen[index] = true;
        let mut cursor = 0;
        let mut liberty = false;
        while cursor < stones.len() {
            for next in self.neighbors(stones[cursor]) {
                if self.cells[next] == 0 {
                    liberty = true;
                } else if !seen[next] && self.cells[next] == self.cells[index] {
                    seen[next] = true;
                    stones.push(next);
                }
            }
            cursor += 1;
        }
        (stones, liberty)
    }

    fn play(&mut self, player: u8, index: usize, allow_suicide: bool) -> bool {
        if self.cells[index] != 0 {
            return false;
        }
        self.cells[index] = player;
        let neighbors: Vec<_> = self.neighbors(index).collect();
        for next in neighbors {
            if self.cells[next] == 3 - player {
                let (stones, liberty) = self.group(next);
                if !liberty {
                    for stone in stones {
                        self.cells[stone] = 0;
                    }
                }
            }
        }
        let (stones, liberty) = self.group(index);
        if !liberty {
            if !allow_suicide {
                return false;
            }
            for stone in stones {
                self.cells[stone] = 0;
            }
        }
        true
    }
}
