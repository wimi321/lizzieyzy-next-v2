#![allow(clippy::unwrap_used, clippy::expect_used)]

use go_core::{BoardSize, Color, Move, Point, Position, Rules};
use proptest::prelude::*;

use crate::*;

fn board(color: Color) -> Position {
    Position::with_setup(
        BoardSize::new(19, 19).unwrap(),
        Rules::from_name("chinese").unwrap(),
        color,
        [],
    )
    .unwrap()
}

fn mv(color: Color, coordinate: &str) -> Move {
    Move::from_gtp(color, coordinate, BoardSize::new(19, 19).unwrap()).unwrap()
}

fn candidate(
    color: Color,
    coordinate: &str,
    order: u32,
    winrate: f64,
    score: Option<f64>,
) -> Candidate {
    let mv = mv(color, coordinate);
    Candidate {
        mv,
        order,
        visits: 100,
        winrate,
        score_lead: score,
        pv: vec![mv],
    }
}

fn input(color: Color) -> PositionInput {
    PositionInput {
        node_id: 10,
        move_number: 12,
        board: board(color),
        root_visits: 500,
        actual_move: Some(mv(color, "Q16")),
        continuation: vec![mv(color, "Q16"), mv(color.opponent(), "D16")],
        candidates: vec![
            candidate(color, "D4", 0, 0.6, Some(3.0)),
            candidate(color, "Q16", 1, 0.5, Some(1.0)),
        ],
    }
}

fn evidence() -> Evidence {
    build_evidence(&[input(Color::Black)], false, Language::En).unwrap()
}

#[test]
fn side_to_play_perspective_and_actual_candidate_loss_are_explicit() {
    let black = build_evidence(&[input(Color::Black)], false, Language::ZhCn).unwrap();
    assert!(black.text.contains("轮到的一方 B"));
    assert!(black.text.contains("winrate=60.0%, scoreLead=3.0"));
    assert!(black.text.contains("胜率损失（百分点）=10.0"));
    assert!(black.text.contains("目差损失（目）=2.0"));
    let mut position = input(Color::White);
    position.candidates[0].winrate = 0.3;
    position.candidates[0].score_lead = Some(-4.0);
    position.candidates[1].winrate = 0.5;
    position.candidates[1].score_lead = Some(-1.0);
    let white = build_evidence(&[position], false, Language::En).unwrap();
    assert!(white.text.contains("side to play W"));
    assert!(white.text.contains("winrate=70.0%, scoreLead=4.0"));
    assert!(white.text.contains("percentage points)=20.0"));
    assert!(white.text.contains("candidate (points)=3.0"));
    assert!(white.text.contains("root visits=500"));
    assert!(white.text.contains("Actual next move：W Q16"));
}

#[test]
fn top_three_plus_actual_are_ordered_and_unknown_loss_is_not_zero() {
    let mut position = input(Color::Black);
    position.candidates = vec![
        candidate(Color::Black, "Q16", 4, 0.1, Some(-8.0)),
        candidate(Color::Black, "D4", 0, 0.6, Some(3.0)),
        candidate(Color::Black, "C3", 3, 0.2, Some(-3.0)),
        candidate(Color::Black, "D16", 1, 0.5, Some(2.0)),
        candidate(Color::Black, "Q4", 2, 0.4, Some(1.0)),
    ];
    let output = build_evidence(&[position.clone()], false, Language::ZhCn).unwrap();
    assert!(output.text.contains("候选 #5: Q16"));
    assert!(!output.text.contains("候选 #4"));
    assert!(!output.allowed_coordinates.contains("C3"));
    assert!(output.text.find("候选 #1").unwrap() < output.text.find("候选 #5").unwrap());
    position.actual_move = Some(mv(Color::Black, "K10"));
    let output = build_evidence(&[position], false, Language::ZhCn).unwrap();
    assert!(output.text.contains("不在候选中，损失未知"));
    assert!(!output.text.contains("损失（目）="));
    assert!(!output.text.contains("损失（百分点）="));
    assert!(output.allowed_coordinates.contains("K10"));
}

#[test]
fn board_groups_liberties_capture_and_illegal_prefix_use_go_core() {
    let size = BoardSize::new(5, 5).unwrap();
    let point = |text| Point::from_gtp(text, size).unwrap();
    let initial = Position::with_setup(
        size,
        Rules::from_name("chinese").unwrap(),
        Color::Black,
        [
            (Color::Black, point("A2")),
            (Color::Black, point("B1")),
            (Color::Black, point("C2")),
            (Color::White, point("B2")),
        ],
    )
    .unwrap();
    let first = Move::Play(Color::Black, point("B3"));
    let mut position = input(Color::Black);
    position.board = initial.clone();
    position.actual_move = Some(first);
    position.continuation = vec![
        first,
        Move::Play(Color::White, point("B2")),
        Move::Play(Color::Black, point("E5")),
    ];
    position.candidates = vec![Candidate {
        mv: first,
        order: 0,
        visits: 3,
        winrate: 0.7,
        score_lead: Some(2.0),
        pv: vec![first],
    }];
    let output = build_evidence(&[position], false, Language::ZhCn).unwrap();
    assert!(output.text.contains("棋盘：5x5"));
    assert!(output.text.contains("W 子=[B2] 气=1 [B3]"));
    assert!(output.text.contains("少气棋块"));
    assert!(output.text.contains("提子=[B2]"));
    assert!(output.text.contains("停止：不合法着手"));
    assert!(!output.allowed_coordinates.contains("E5"));
    assert_eq!(initial.stone_at(point("B2")), Some(Color::White));
    assert_eq!(initial.liberties_at(point("B2")), Some(1));
}

#[test]
fn local_replay_retains_ko_history_and_uses_the_selected_rules() {
    for rules in ["chinese", "tromp-taylor", "new-zealand"] {
        let size = BoardSize::new(5, 5).unwrap();
        let black = [(0, 2), (1, 1), (1, 3)].map(|(x, y)| (Color::Black, Point { x, y }));
        let white = [(1, 2), (2, 1), (2, 3), (3, 2)].map(|(x, y)| (Color::White, Point { x, y }));
        let mut board = Position::with_setup(
            size,
            Rules::from_name(rules).unwrap(),
            Color::Black,
            black.into_iter().chain(white),
        )
        .unwrap();
        board
            .play(Move::Play(Color::Black, Point { x: 2, y: 2 }))
            .unwrap();
        let mut position = input(Color::White);
        position.board = board;
        position.actual_move = None;
        position.continuation.clear();
        position.candidates = vec![Candidate {
            mv: Move::Pass(Color::White),
            order: 0,
            visits: 10,
            winrate: 0.5,
            score_lead: Some(0.0),
            pv: vec![
                Move::Pass(Color::White),
                Move::Pass(Color::Black),
                Move::Play(Color::White, Point { x: 1, y: 2 }),
            ],
        }];
        let output = build_evidence(&[position], false, Language::En).unwrap();
        if rules == "chinese" {
            assert!(output.text.contains("3.W B3 captures=[C3]"));
            assert!(!output.text.contains("STOPPED"));
        } else {
            assert!(output.text.contains("STOPPED"));
            assert!(output.text.contains("repeats a position"));
        }
    }
}

#[test]
fn replay_respects_turns_bounds_pass_and_limits() {
    let mut position = input(Color::Black);
    let moves: Vec<_> = (0..16)
        .map(|x| {
            Move::Play(
                if x % 2 == 0 {
                    Color::Black
                } else {
                    Color::White
                },
                Point { x, y: 8 },
            )
        })
        .collect();
    position.actual_move = Some(moves[0]);
    position.continuation = moves.clone();
    position.candidates = vec![Candidate {
        mv: moves[0],
        order: 0,
        visits: 1,
        winrate: 0.5,
        score_lead: None,
        pv: moves,
    }];
    let output = build_evidence(&[position.clone()], false, Language::En).unwrap();
    assert!(output.text.contains("5.B E11"));
    assert!(output.text.contains("12.W M11"));
    assert!(!output.allowed_coordinates.contains("N11"));
    let start = output.text.find("Recorded continuation").unwrap();
    let stop = output.text[start..].find("Candidate local replay").unwrap() + start;
    assert!(!output.text[start..stop].contains("6.W F11"));
    position.candidates[0].pv[1] = Move::Pass(Color::Black);
    let output = build_evidence(&[position.clone()], false, Language::En).unwrap();
    assert!(output.text.contains("wrong player"));
    position.candidates[0].pv[1] = Move::Play(Color::White, Point { x: 24, y: 0 });
    assert!(
        build_evidence(&[position], false, Language::En)
            .unwrap()
            .text
            .contains("out-of-bounds")
    );
    let mut position = input(Color::Black);
    position.actual_move = Some(Move::Pass(Color::Black));
    position.continuation = vec![Move::Pass(Color::Black), mv(Color::White, "D16")];
    assert!(
        build_evidence(&[position], false, Language::En)
            .unwrap()
            .text
            .contains("1.B pass; 2.W D16")
    );
}

#[test]
fn key_selection_enforces_limit_spacing_importance_and_chronology() {
    let inputs: Vec<_> = (0..30)
        .map(|number| {
            let mut position = input(Color::Black);
            position.node_id = number;
            position.move_number = number;
            position.candidates[1].score_lead = Some(3.0 - f64::from(number));
            position
        })
        .collect();
    let output = build_evidence(&inputs, true, Language::En).unwrap();
    assert_eq!(
        (output.analyzed, output.selected, output.omitted),
        (30, 5, 25)
    );
    assert_eq!(
        output
            .positions
            .iter()
            .map(|position| position.move_number)
            .collect::<Vec<_>>(),
        [17, 20, 23, 26, 29]
    );
    let small = build_evidence(&inputs[..3], true, Language::En).unwrap();
    assert_eq!(small.positions.len(), 1);
    assert_eq!(small.positions[0].move_number, 2);
    let mut unknown = input(Color::Black);
    unknown.move_number = 100;
    unknown.candidates[0].score_lead = None;
    unknown.candidates[1].score_lead = None;
    unknown.candidates[0].winrate = 1.0;
    unknown.candidates[1].winrate = 0.0;
    let mut score = input(Color::Black);
    score.move_number = 101;
    score.candidates[1].score_lead = Some(-6.0); // importance 9, versus 100 / 10.
    let output = build_evidence(&[unknown, score], true, Language::En).unwrap();
    assert_eq!(output.positions[0].move_number, 100);
}

#[test]
fn selection_visits_fallback_ties_and_missing_score_are_preserved() {
    let mut earlier = input(Color::Black);
    earlier.actual_move = None;
    earlier.move_number = 0;
    let mut later = earlier.clone();
    later.move_number = 1;
    later.candidates[0].visits = 1_000_000;
    assert_eq!(
        build_evidence(&[earlier.clone(), later.clone()], true, Language::En)
            .unwrap()
            .positions[0]
            .move_number,
        1
    );
    earlier.candidates[0].visits = 1_000_000;
    assert_eq!(
        build_evidence(&[later, earlier], true, Language::En)
            .unwrap()
            .positions[0]
            .move_number,
        0
    );
    let mut no_score = input(Color::Black);
    for candidate in &mut no_score.candidates {
        candidate.score_lead = None;
    }
    let text = build_evidence(&[no_score], false, Language::En)
        .unwrap()
        .text;
    assert!(text.contains("scoreLead=unknown"));
    assert!(!text.contains("score loss"));
    assert!(text.contains("winrate loss"));
}

#[test]
fn verifier_allows_only_supplied_moves_and_named_groups_not_all_stones_or_liberties() {
    let mut position = input(Color::Black);
    let size = position.board.size();
    position.board = Position::with_setup(
        size,
        Rules::from_name("chinese").unwrap(),
        Color::Black,
        [
            (Color::Black, Point::from_gtp("C4", size).unwrap()),
            (Color::White, Point::from_gtp("K10", size).unwrap()),
            (Color::White, Point::from_gtp("A19", size).unwrap()),
        ],
    )
    .unwrap();
    let output = build_evidence(&[position], false, Language::En).unwrap();
    assert!(output.text.contains("K10"));
    assert!(verify("D4 connects C4; Q16 follows D16. 100%", &output).is_empty());
    let issues = verify("K10, A19, C5 and A20. 100.1% 999 101 ％", &output);
    assert_eq!(issues.len(), 6);
    assert!(issues.iter().any(|issue| issue.contains("K10")));
    assert!(issues.iter().any(|issue| issue.contains("C5")));
    assert!(
        verify(
            "k10, wordK10 and K10word are not coordinate tokens; 99.9 %.",
            &output
        )
        .is_empty()
    );
    let mut empty = output;
    empty.allowed_coordinates.clear();
    assert_eq!(verify("D4", &empty).len(), 1);
}

#[test]
fn verifier_rejects_invalid_uppercase_coordinate_tokens_without_checking_lowercase() {
    let evidence = evidence();
    for coordinate in ["I4", "A0", "Z26", "A999", "A01", "B2026"] {
        let issues = verify(&format!("Try {coordinate}."), &evidence);
        assert_eq!(issues.len(), 1, "{coordinate}");
        assert!(issues[0].starts_with("Invalid coordinate"));
        assert!(issues[0].contains(coordinate));
    }
    assert!(verify("i4 a0 z26 a999 wordI4 I4word AI4 No.1", &evidence).is_empty());
}

#[test]
fn chinese_english_and_requested_languages_include_teaching_preferences() {
    let evidence = evidence();
    for language in [
        Language::ZhCn,
        Language::ZhTw,
        Language::En,
        Language::Ja,
        Language::Ko,
        Language::Th,
    ] {
        let preferences = Preferences {
            rank: "9d".into(),
            style: Style::Patient,
            terminology: Terminology::Few,
            detail: Detail::Detailed,
            variations: Variations::Many,
        };
        let messages =
            build_prompt(&evidence, Mode::NextMove, language, &preferences, None).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert!(messages[1].content.ends_with(&evidence.text));
        let system = &messages[0].content;
        assert!(system.contains("9d"));
        let required = match language {
            Language::ZhCn => ["简体中文", "耐心", "术语=少", "详略=详细", "变化=多"],
            Language::ZhTw => ["繁體中文", "耐心", "术语=少", "详略=详细", "变化=多"],
            Language::En => ["English", "Patient", "Few", "Detailed", "Many"],
            Language::Ja => ["Japanese", "Patient", "Few", "Detailed", "Many"],
            Language::Ko => ["Korean", "Patient", "Few", "Detailed", "Many"],
            Language::Th => ["Thai", "Patient", "Few", "Detailed", "Many"],
        };
        for text in required {
            assert!(system.contains(text), "{text}");
        }
        assert!(system.contains("0.5") && system.contains("1.5") && system.contains("2–4"));
    }
}

#[test]
fn follow_up_is_four_messages_bounded_by_unicode_characters_and_total_prompt_limit() {
    let evidence = evidence();
    let previous = format!("omit{}", "棋".repeat(12_000));
    let messages = build_prompt(
        &evidence,
        Mode::Range,
        Language::ZhCn,
        &Preferences::default(),
        Some(FollowUp {
            previous_answer: &previous,
            question: "为什么？",
        }),
    )
    .unwrap();
    assert_eq!(
        messages
            .iter()
            .map(|message| message.role)
            .collect::<Vec<_>>(),
        ["system", "user", "assistant", "user"]
    );
    assert_eq!(messages[2].content.chars().count(), 12_000);
    assert!(!messages[2].content.contains("omit"));
    assert!(messages[3].content.contains("为什么？"));
    let oversized = "棋".repeat(MAX_PROMPT_CHARACTERS);
    assert_eq!(
        build_prompt(
            &evidence,
            Mode::WholeGame,
            Language::En,
            &Preferences::default(),
            Some(FollowUp {
                previous_answer: "first",
                question: &oversized
            })
        ),
        Err(Error::PromptTooLong)
    );
    let mut evidence = evidence;
    evidence.text.clear();
    let overhead: usize = build_prompt(
        &evidence,
        Mode::NextMove,
        Language::En,
        &Preferences::default(),
        None,
    )
    .unwrap()
    .iter()
    .map(|message| message.content.chars().count())
    .sum();
    evidence.text = "棋".repeat(MAX_PROMPT_CHARACTERS - overhead);
    assert!(
        build_prompt(
            &evidence,
            Mode::NextMove,
            Language::En,
            &Preferences::default(),
            None
        )
        .is_ok()
    );
    evidence.text.push('棋');
    assert_eq!(
        build_prompt(
            &evidence,
            Mode::NextMove,
            Language::En,
            &Preferences::default(),
            None
        ),
        Err(Error::PromptTooLong)
    );
}

#[test]
fn repair_lists_only_eight_issues_and_uses_full_draft_with_bound() {
    let prompt = build_prompt(
        &evidence(),
        Mode::NextMove,
        Language::En,
        &Preferences::default(),
        None,
    )
    .unwrap();
    let issues = (0..12)
        .map(|index| format!("issue-{index:02}"))
        .collect::<Vec<_>>();
    let corrected = build_correction(&prompt, "D4 draft", &issues, Language::En).unwrap();
    assert_eq!(corrected[2].role, "assistant");
    assert_eq!(corrected[2].content, "D4 draft");
    assert!(corrected[3].content.contains("issue-07"));
    assert!(!corrected[3].content.contains("issue-08"));
    assert_eq!(
        build_correction(
            &prompt,
            &"a".repeat(MAX_PROMPT_CHARACTERS),
            &issues,
            Language::En
        ),
        Err(Error::PromptTooLong)
    );
}

#[test]
fn rejects_bad_analysis_and_preferences_without_inventing_analysis() {
    assert_eq!(
        build_evidence(&[], true, Language::En),
        Err(Error::NoAnalysis)
    );
    let mut position = input(Color::Black);
    position.candidates.clear();
    assert_eq!(
        build_evidence(&[position], false, Language::En),
        Err(Error::NoAnalysis)
    );
    for value in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        let mut position = input(Color::Black);
        position.candidates[0].winrate = value;
        assert_eq!(
            build_evidence(&[position], false, Language::En),
            Err(Error::InvalidAnalysis)
        );
    }
    for number in 1..=18 {
        assert!(
            Preferences {
                rank: format!("{number}k"),
                ..Preferences::default()
            }
            .validate()
            .is_ok()
        );
    }
    for number in 1..=9 {
        assert!(
            Preferences {
                rank: format!("{number}d"),
                ..Preferences::default()
            }
            .validate()
            .is_ok()
        );
    }
    for rank in ["0k", "19k", "10d", "01k", "-1d", "5K", "", "9999d"] {
        assert_eq!(
            Preferences {
                rank: rank.into(),
                ..Preferences::default()
            }
            .validate(),
            Err(Error::InvalidPreferences)
        );
    }
}

#[test]
fn java_comment_golden_decodes_and_replacement_preserves_user_text() {
    // Handwritten golden of TeacherCommentCodec's wire format, not a live model response.
    let java = include_str!("../fixtures/java-comment.txt").trim_end();
    let decoded = codec::decode(java).unwrap();
    assert_eq!(decoded.generated_at, "2026-10-20T12:34:56Z");
    assert_eq!(decoded.model, "local-model");
    assert_eq!(decoded.body, "Try D4 before extending at Q16.");
    let updated = codec::upsert(java, "New D4 explanation", "new-model", "now").unwrap();
    assert_eq!(codec::remove_blocks(&updated), "User comment");
    assert_eq!(updated.matches(codec::BEGIN).count(), 1);
    assert!(!updated.contains("Try D4"));
    let original = format!(
        "  User\r\ntext  \n\n{}\nUser suffix",
        codec::encode("old", "m", "t").unwrap()
    );
    let updated = codec::upsert(&original, "new", "m", "t").unwrap();
    assert_eq!(
        codec::remove_blocks(&updated),
        "  User\r\ntext  \nUser suffix"
    );
}

#[test]
fn follow_up_appends_in_same_block_and_markers_cannot_create_blocks() {
    let original = codec::upsert("User comment", "Original explanation", "m", "first").unwrap();
    let answer = format!("New answer {} fake {}", codec::BEGIN, codec::END);
    let followed = codec::append_follow_up(&original, "Why?", &answer, "m", "second").unwrap();
    let followed =
        codec::append_follow_up(&followed, "And next?", "Another answer", "m", "third").unwrap();
    assert_eq!(followed.matches(codec::BEGIN).count(), 1);
    assert_eq!(followed.matches(codec::END).count(), 1);
    let decoded = codec::decode(&followed).unwrap();
    assert!(decoded.body.starts_with("Original explanation"));
    assert!(decoded.body.contains("追问：Why?"));
    assert!(decoded.body.contains("追问：And next?"));
    assert!(decoded.body.contains("BEGIN escaped"));
    assert_eq!(codec::remove_blocks(&followed), "User comment");
    assert_eq!(
        codec::append_follow_up("no block", "why", "answer", "m", "t"),
        Err(Error::MissingCommentary)
    );
}

#[test]
fn codec_bounds_unicode_and_roundtrips_varied_text_without_losing_user_comments() {
    assert_eq!(
        codec::encode("\r\n  ", "m", "t"),
        Err(Error::EmptyCommentary)
    );
    assert!(codec::encode(&"棋".repeat(MAX_ANSWER_CHARACTERS), "m", "t").is_ok());
    assert_eq!(
        codec::encode(&"棋".repeat(MAX_ANSWER_CHARACTERS + 1), "m", "t"),
        Err(Error::CommentaryTooLong)
    );
    let pieces = [
        "汉字😀",
        "brackets [ ]",
        "line\nnext",
        "carriage\r\nnext",
        codec::BEGIN,
        codec::END,
        "\\",
    ];
    for first in pieces {
        for second in pieces {
            let body = format!("{first}/{second}");
            let original = " \r\nUser [text] and 👋\n";
            let encoded = codec::upsert(original, &body, "model\nname", "timestamp").unwrap();
            let decoded = codec::decode(&encoded).unwrap();
            assert_eq!(
                decoded.body,
                body.replace("\r\n", "\n")
                    .replace(codec::BEGIN, "[LizzieYzy AI Commentary BEGIN escaped]")
                    .replace(codec::END, "[LizzieYzy AI Commentary END escaped]")
            );
            assert_eq!(decoded.model, "model name");
            assert_eq!(codec::remove_blocks(&encoded), original);
        }
    }
    let unfinished = format!("user {} unfinished", codec::BEGIN);
    assert_eq!(codec::remove_blocks(&unfinished), unfinished);
    let two = format!(
        "{}\n\n{}",
        codec::encode("first", "m", "t").unwrap(),
        codec::encode("second", "m", "t").unwrap()
    );
    assert_eq!(codec::decode(&two).unwrap().body, "second");
    assert_eq!(codec::remove_blocks(&two), "");
}

proptest! {
    #[test]
    fn comment_codec_property_roundtrip_preserves_user_text(
        prefix in "[a-z汉字😀\\n\\r ]{0,120}",
        body in "[a-zA-Z汉字😀0-9\\n\\r\\[\\] ]{1,500}",
        marker in prop::sample::select(vec![codec::BEGIN, codec::END, ""]),
    ) {
        let body = format!("text {body}{marker}");
        let encoded = codec::upsert(&prefix, &body, "local-model", "2026-10-20T12:34:56Z").unwrap();
        let decoded = codec::decode(&encoded).unwrap();
        let normalized = body.replace("\r\n", "\n").replace('\r', "\n")
            .replace(codec::BEGIN, "[LizzieYzy AI Commentary BEGIN escaped]")
            .replace(codec::END, "[LizzieYzy AI Commentary END escaped]");
        prop_assert_eq!(decoded.body, normalized.trim());
        prop_assert_eq!(codec::remove_blocks(&encoded), prefix);
        prop_assert_eq!(encoded.matches(codec::BEGIN).count(), 1);
        prop_assert_eq!(encoded.matches(codec::END).count(), 1);
    }

    #[test]
    fn coordinate_verifier_property_checks_the_entire_board(width in 2u8..=25, height in 2u8..=25, x in 0u8..25, y in 0u8..25) {
        let size = BoardSize::new(width, height).unwrap();
        let point = Point { x: x % width, y: y % height };
        let target = Move::Play(Color::Black, point);
        let position = PositionInput {
            node_id: 1, move_number: 0,
            board: Position::new(size, Rules::from_name("chinese").unwrap()).unwrap(),
            root_visits: 1, actual_move: None, continuation: vec![],
            candidates: vec![Candidate { mv: target, order: 0, visits: 1, winrate: 0.5, score_lead: None, pv: vec![target] }],
        };
        let evidence = build_evidence(&[position], false, Language::En).unwrap();
        for other_y in 0..height {
            for other_x in 0..width {
                let other = Point { x: other_x, y: other_y };
                let coordinate = other.to_gtp(size).unwrap();
                prop_assert_eq!(verify(&coordinate, &evidence).is_empty(), other == point);
            }
        }
    }
}
