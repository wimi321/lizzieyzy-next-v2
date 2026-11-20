use document::lz::{self, SavedAnalysis, SavedCandidate};
use go_core::{BoardSize, Color, Move, Point, Position, RulePreset};
use sgf::{Collection, GameTree, Node, Property};

use super::SupportError;

const SEED: u64 = 0x5033_3434_4245_4e43;
const SIZE: BoardSize = BoardSize {
    width: 19,
    height: 19,
};

/// A reproducible synthetic reviewed game: 400 main-line moves, with a
/// ten-move variation after each of moves 10, 20, ..., 300.
pub fn reviewed_game() -> Result<String, SupportError> {
    let mut rng = Generator(SEED);
    let mut position = Position::new(SIZE, RulePreset::Chinese.rules())?;
    let mut main = Vec::with_capacity(400);
    let mut branches = Vec::with_capacity(30);
    for number in 1..=400 {
        main.push(reviewed_move(&mut position, &mut rng, number)?);
        if number <= 300 && number % 10 == 0 {
            let mut variation = position.clone();
            let mut sequence = Vec::with_capacity(10);
            for offset in 1..=10 {
                sequence.push(reviewed_move(&mut variation, &mut rng, number + offset)?);
            }
            branches.push(GameTree {
                sequence,
                children: Vec::new(),
            });
        }
    }
    let mut tail = GameTree {
        sequence: main.split_off(300),
        children: Vec::new(),
    };
    for branch in branches.into_iter().rev() {
        tail = GameTree {
            sequence: main.split_off(main.len() - 10),
            children: vec![tail, branch],
        };
    }
    serialize_checked(GameTree {
        sequence: vec![root("Reviewed benchmark game")],
        children: vec![tail],
    })
}

/// A shallow, heavily branching study tree with exactly 20,000 nodes,
/// including the metadata root. Every path contains legal Go moves.
pub fn large_tree() -> Result<String, SupportError> {
    let mut rng = Generator(SEED ^ 0x7472_6565);
    let position = Position::new(SIZE, RulePreset::Chinese.rules())?;
    serialize_checked(GameTree {
        sequence: vec![root("Large benchmark study tree")],
        children: children(&position, &mut rng, 19_999)?,
    })
}

fn children(
    position: &Position,
    rng: &mut Generator,
    nodes: usize,
) -> Result<Vec<GameTree>, SupportError> {
    let count = nodes.min(4);
    let mut result = Vec::with_capacity(count);
    for index in 0..count {
        let branch_nodes = nodes / count + usize::from(index < nodes % count);
        let mut next = position.clone();
        let mv = rng.play(&mut next)?;
        result.push(GameTree {
            sequence: vec![move_node(mv)?],
            children: children(&next, rng, branch_nodes - 1)?,
        });
    }
    Ok(result)
}

fn root(name: &str) -> Node {
    Node {
        properties: [
            ("GM", "1"),
            ("FF", "4"),
            ("CA", "UTF-8"),
            ("SZ", "19"),
            ("KM", "7.5"),
            ("RU", "Chinese"),
            ("AP", "LizzieYzy-Next-2:performance"),
            ("GN", name),
        ]
        .into_iter()
        .map(|(key, value)| property(key, value.into()))
        .collect(),
    }
}

fn move_node(mv: Move) -> Result<Node, SupportError> {
    Ok(Node {
        properties: vec![property(
            if mv.color() == Color::Black { "B" } else { "W" },
            mv.to_sgf(SIZE)?,
        )],
    })
}

fn reviewed_move(
    position: &mut Position,
    rng: &mut Generator,
    number: usize,
) -> Result<Node, SupportError> {
    let mv = rng.play(position)?;
    let mut node = move_node(mv)?;
    node.properties.push(property(
        "C",
        format!(
            "Review move {number}: compare the candidates and the local continuation.\n\
             Synthetic analysis with a fixed seed; brackets [variation] and \\ escapes."
        ),
    ));
    let mut candidates = Vec::with_capacity(2);
    for order in 0..2 {
        let mut preview = position.clone();
        let mut pv = Vec::with_capacity(4);
        for _ in 0..4 {
            pv.push(rng.play(&mut preview)?.to_gtp(SIZE)?);
        }
        if candidates
            .iter()
            .any(|candidate: &SavedCandidate| candidate.coordinate == pv[0])
        {
            continue;
        }
        candidates.push(SavedCandidate {
            coordinate: pv[0].clone(),
            visits: 750 - 500 * order as u64,
            winrate: 0.52 - f64::from(order) * 0.01,
            score_lead: Some(1.5 - f64::from(order)),
            score_stdev: Some(8.0),
            prior: 0.3 - f64::from(order) * 0.1,
            order,
            edge_visits: Some(100),
            pv,
            pv_visits: vec![100, 90, 80, 70],
        });
    }
    let analysis = SavedAnalysis {
        engine_name: "KataGo-benchmark-synthetic".into(),
        winrate: 0.52,
        visits: 1_000,
        score_lead: Some(1.5),
        score_stdev: Some(8.0),
        pda: Some(0.0),
        wrn: Some(1.0),
        candidates,
        ownership: Some(
            position
                .stones()
                .iter()
                .map(|stone| match stone {
                    Some(Color::Black) => 0.8,
                    Some(Color::White) => -0.8,
                    None => 0.0,
                })
                .collect(),
        ),
    };
    node.properties.push(property(
        "LZ",
        lz::format(&analysis, SIZE, position.to_play())?,
    ));
    Ok(node)
}

fn property(identifier: &str, value: String) -> Property {
    Property {
        identifier: identifier.into(),
        values: vec![value],
    }
}

fn serialize_checked(game: GameTree) -> Result<String, SupportError> {
    let text = sgf::serialize(&Collection {
        games: vec![game],
        encoding: "UTF-8".into(),
        warnings: Vec::new(),
    });
    let parsed = sgf::parse(text.as_bytes())?;
    if parsed.games.len() != 1 || !parsed.warnings.is_empty() {
        return Err(SupportError::Invalid(
            "generated SGF must contain one game without parse warnings".into(),
        ));
    }
    Ok(text)
}

/// Explicit SplitMix64 arithmetic keeps fixtures stable across platforms and
/// dependency updates. This is test data, not a cryptographic random source.
struct Generator(u64);

impl Generator {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn play(&mut self, position: &mut Position) -> Result<Move, SupportError> {
        let color = position.to_play();
        let mut points: Vec<_> = (0..SIZE.area()).collect();
        for end in (1..points.len()).rev() {
            points.swap(end, (self.next() % (end as u64 + 1)) as usize);
        }
        for index in points {
            let point = Point {
                x: (index % usize::from(SIZE.width)) as u8,
                y: (index / usize::from(SIZE.width)) as u8,
            };
            if position.stone_at(point).is_some() {
                continue;
            }
            let mv = Move::Play(color, point);
            if position.play(mv).is_ok() {
                return Ok(mv);
            }
        }
        let mv = Move::Pass(color);
        position.play(mv)?;
        Ok(mv)
    }
}
