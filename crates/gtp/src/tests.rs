#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use go_core::{BoardSize, Color, Move, Point};
use proptest::prelude::*;

#[test]
fn golden_single_multi_id_error_empty_and_genmove() {
    // Constructed from GTP 2 framing, not a recording of a real engine.
    let mut decoder = Decoder::default();
    let replies = include_str!("../fixtures/responses.gtp")
        .lines()
        .filter_map(|line| decoder.push_line(line).unwrap())
        .collect::<Vec<_>>();
    decoder.finish().unwrap();
    assert_eq!(replies.len(), 8);
    assert_eq!(
        replies[0],
        Response {
            id: None,
            success: true,
            body: "2".into()
        }
    );
    assert_eq!(replies[1].id, Some(17));
    assert_eq!(
        replies[2].body,
        "protocol_version\nname\nversion\nlist_commands\nplay\ngenmove"
    );
    assert!(!replies[3].success);
    assert_eq!(replies[3].body, "unknown command");
    assert_eq!(replies[4].body, "");
    let size = BoardSize::new(19, 19).unwrap();
    assert_eq!(
        parse_genmove(&replies[5].body, size).unwrap(),
        Genmove::Pass
    );
    assert_eq!(
        parse_genmove(&replies[6].body, size).unwrap(),
        Genmove::Resign
    );
    assert_eq!(
        parse_genmove(&replies[7].body, size).unwrap(),
        Genmove::Play(Point { x: 15, y: 3 })
    );
    for reply in replies {
        assert_eq!(parse_response(&reply.encode().unwrap()).unwrap(), reply);
    }
}

#[test]
fn standard_commands_use_go_coordinates_and_optional_ids() {
    let size = BoardSize::new(19, 19).unwrap();
    let commands = [
        (StandardCommand::ProtocolVersion, "protocol_version"),
        (StandardCommand::Name, "name"),
        (StandardCommand::Version, "version"),
        (
            StandardCommand::KnownCommand("undo".into()),
            "known_command undo",
        ),
        (StandardCommand::ListCommands, "list_commands"),
        (StandardCommand::BoardSize(19), "boardsize 19"),
        (StandardCommand::ClearBoard, "clear_board"),
        (StandardCommand::Komi(7.5), "komi 7.5"),
        (StandardCommand::FixedHandicap(2), "fixed_handicap 2"),
        (
            StandardCommand::PlaceFreeHandicap(2),
            "place_free_handicap 2",
        ),
        (
            StandardCommand::SetFreeHandicap {
                points: vec![Point { x: 3, y: 3 }, Point { x: 15, y: 15 }],
                size,
            },
            "set_free_handicap D16 Q4",
        ),
        (
            StandardCommand::Play {
                mv: Move::Play(Color::White, Point { x: 15, y: 3 }),
                size,
            },
            "play w Q16",
        ),
        (
            StandardCommand::Play {
                mv: Move::Pass(Color::Black),
                size,
            },
            "play b pass",
        ),
        (StandardCommand::Genmove(Color::White), "genmove w"),
        (StandardCommand::Undo, "undo"),
        (
            StandardCommand::TimeSettings {
                main: 60,
                period: 30,
                stones: 1,
            },
            "time_settings 60 30 1",
        ),
        (
            StandardCommand::TimeLeft {
                color: Color::Black,
                seconds: 50,
                stones: 0,
            },
            "time_left b 50 0",
        ),
        (StandardCommand::FinalScore, "final_score"),
        (StandardCommand::Quit, "quit"),
        (
            StandardCommand::KataSetRules("chinese".into()),
            "kata-set-rules chinese",
        ),
        (
            StandardCommand::KataTimeSettings(vec!["absolute".into(), "300".into()]),
            "kata-time_settings absolute 300",
        ),
    ];
    for (command, expected) in commands {
        assert_eq!(command.encode(None).unwrap(), expected);
        assert_eq!(command.encode(Some(42)).unwrap(), format!("42 {expected}"));
    }
}

#[test]
fn malformed_inputs_fail_without_synthesizing_moves() {
    for text in [
        "hello\n\n",
        "=12x text\n\n",
        "=4294967296 value\n\n",
        "= text",
        "= text\n",
        "= value\0\n\n",
        "= ok\n\n= twice\n\n",
    ] {
        assert!(parse_response(text).is_err(), "{text:?}");
    }
    for text in [
        "",
        "17",
        "4294967296 name",
        "play b D4\nquit",
        "play b D4\rquit",
        "name\0",
    ] {
        assert!(Command::parse(text).is_err(), "{text:?}");
    }
    let size = BoardSize::new(9, 9).unwrap();
    for text in ["", "D4 D5", "I4", "D0", "D10", "Q16", "resignation"] {
        assert!(parse_genmove(text, size).is_err());
    }
    assert!(StandardCommand::Komi(f64::NAN).encode(None).is_err());
    assert!(StandardCommand::BoardSize(1).encode(None).is_err());
    assert!(
        StandardCommand::TimeSettings {
            main: u32::MAX,
            period: 1,
            stones: 1
        }
        .encode(None)
        .is_err()
    );
    assert!(
        StandardCommand::TimeLeft {
            color: Color::Black,
            seconds: u32::MAX,
            stones: 0
        }
        .encode(None)
        .is_err()
    );
    assert!(Command::parse(&"x".repeat(MAX_COMMAND_BYTES + 1)).is_err());
    let mut decoder = Decoder::default();
    assert!(
        decoder
            .push_line(&format!("= {}", "x".repeat(MAX_RESPONSE_BYTES)))
            .is_err()
    );
    assert_eq!(
        parse_response("=42 hello\r\nworld\r\n\r\n").unwrap().body,
        "hello\nworld"
    );
}

proptest! {
    #[test]
    fn standard_time_commands_always_encode_integer_fields(main in 0u32..=i32::MAX as u32, period in 0u32..=i32::MAX as u32, stones in 0u32..=i32::MAX as u32) {
        let settings = StandardCommand::TimeSettings { main, period, stones }.encode(None).unwrap();
        let settings = Command::parse(&settings).unwrap();
        prop_assert_eq!(settings.arguments.iter().map(|value| value.parse::<u32>().unwrap()).collect::<Vec<_>>(), vec![main, period, stones]);
        let remaining = StandardCommand::TimeLeft { color: Color::Black, seconds: main, stones }.encode(None).unwrap();
        let remaining = Command::parse(&remaining).unwrap();
        prop_assert_eq!(remaining.arguments[1].parse::<u32>().unwrap(), main);
        prop_assert_eq!(remaining.arguments[2].parse::<u32>().unwrap(), stones);
    }
    #[test]
    fn command_roundtrip(id in proptest::option::of(any::<u32>()), name in "[a-z][a-z_-]{0,20}", args in prop::collection::vec("[A-Za-z0-9.+-]{1,20}", 0..12)) {
        let command = Command { id, name, arguments: args };
        prop_assert_eq!(Command::parse(&command.encode().unwrap()).unwrap(), command);
    }
    #[test]
    fn response_roundtrip(id in proptest::option::of(any::<u32>()), success in any::<bool>(), body in prop::collection::vec("[A-Za-z0-9 _.-]{1,30}", 1..10)) {
        let body = body.into_iter().map(|line| format!("x{line}")).collect::<Vec<_>>().join("\n");
        let response = Response { id, success, body };
        prop_assert_eq!(parse_response(&response.encode().unwrap()).unwrap(), response);
    }
    #[test]
    fn coordinates_roundtrip(x in 0u8..19, y in 0u8..19, black in any::<bool>()) {
        let size = BoardSize::new(19,19).unwrap();
        let point = Point { x, y };
        let color = if black { Color::Black } else { Color::White };
        let wire = StandardCommand::Play { mv: Move::Play(color, point), size }.encode(None).unwrap();
        let command = Command::parse(&wire).unwrap();
        prop_assert_eq!(parse_genmove(&command.arguments[1],size).unwrap(), Genmove::Play(point));
    }
    #[test]
    fn arbitrary_reply_never_panics(text in any::<String>()) { let _ = parse_response(&text); }
}
