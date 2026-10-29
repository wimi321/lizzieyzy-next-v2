#![allow(clippy::unwrap_used)]

use super::*;
use proptest::prelude::*;

#[test]
fn coordinates_round_trip_on_standard_sizes_and_rectangles() {
    for (width, height) in [(9, 9), (13, 13), (19, 19), (2, 25), (25, 2), (25, 25)] {
        let size = BoardSize::new(width, height).unwrap();
        for y in 0..height {
            for x in 0..width {
                let point = Point { x, y };
                let gtp = point.to_gtp(size).unwrap();
                let sgf = point.to_sgf(size).unwrap();
                assert_eq!(Point::from_gtp(&gtp, size).unwrap(), point);
                assert_eq!(Point::from_sgf(&sgf, size).unwrap(), point);
                for color in [Color::Black, Color::White] {
                    let mv = Move::Play(color, point);
                    assert_eq!(
                        Move::from_gtp(color, &mv.to_gtp(size).unwrap(), size).unwrap(),
                        mv
                    );
                    assert_eq!(
                        Move::from_sgf(color, &mv.to_sgf(size).unwrap(), size).unwrap(),
                        mv
                    );
                }
            }
        }
    }
}

#[test]
fn gtp_skips_i_and_uses_bottom_origin() {
    let size = BoardSize::new(19, 19).unwrap();
    for (point, gtp, sgf) in [
        (Point { x: 0, y: 0 }, "A19", "aa"),
        (Point { x: 0, y: 18 }, "A1", "as"),
        (Point { x: 7, y: 0 }, "H19", "ha"),
        (Point { x: 8, y: 0 }, "J19", "ia"),
        (Point { x: 15, y: 3 }, "Q16", "pd"),
        (Point { x: 18, y: 18 }, "T1", "ss"),
    ] {
        assert_eq!(point.to_gtp(size).unwrap(), gtp);
        assert_eq!(Point::from_gtp(gtp, size).unwrap(), point);
        assert_eq!(Point::from_gtp(&gtp.to_lowercase(), size).unwrap(), point);
        assert_eq!(point.to_sgf(size).unwrap(), sgf);
        assert_eq!(Point::from_sgf(sgf, size).unwrap(), point);
    }
    assert!(Point::from_gtp("I19", size).is_err());
    assert!(Point::from_gtp("i19", size).is_err());
}

#[test]
fn rectangle_uses_height_for_gtp_rows_and_supports_z_column() {
    let size = BoardSize::new(25, 9).unwrap();
    assert_eq!(Point { x: 24, y: 0 }.to_gtp(size).unwrap(), "Z9");
    assert_eq!(Point { x: 24, y: 8 }.to_gtp(size).unwrap(), "Z1");
    assert_eq!(Point::from_gtp("Z9", size).unwrap(), Point { x: 24, y: 0 });
    assert_eq!(Point { x: 24, y: 8 }.to_sgf(size).unwrap(), "yi");
    assert!(Point::from_gtp("A10", size).is_err());
}

#[test]
fn pass_round_trips_with_color_and_normalizes_legacy_sgf() {
    for side in [9, 13, 19] {
        let size = BoardSize::new(side, side).unwrap();
        for color in [Color::Black, Color::White] {
            let mv = Move::Pass(color);
            assert_eq!(mv.to_gtp(size).unwrap(), "pass");
            assert_eq!(mv.to_sgf(size).unwrap(), "");
            for value in ["pass", "PASS", "PaSs"] {
                assert_eq!(Move::from_gtp(color, value, size).unwrap(), mv);
                assert_eq!(
                    Point::from_gtp(value, size),
                    Err(CoordinateError::PassHasNoPoint)
                );
            }
            for value in ["", "tt"] {
                assert_eq!(Move::from_sgf(color, value, size).unwrap(), mv);
                assert_eq!(
                    Point::from_sgf(value, size),
                    Err(CoordinateError::PassHasNoPoint)
                );
            }
        }
    }
}

#[test]
fn legacy_tt_does_not_hide_out_of_bounds_or_real_points_on_large_boards() {
    for (width, height) in [(20, 20), (25, 25)] {
        let size = BoardSize::new(width, height).unwrap();
        assert_eq!(Point::from_sgf("tt", size).unwrap(), Point { x: 19, y: 19 });
        assert_eq!(
            Move::from_sgf(Color::White, "tt", size).unwrap(),
            Move::Play(Color::White, Point { x: 19, y: 19 })
        );
    }
    for (width, height) in [(19, 20), (20, 19), (9, 25), (25, 9)] {
        let size = BoardSize::new(width, height).unwrap();
        assert!(Move::from_sgf(Color::White, "tt", size).is_err());
        assert_eq!(
            Move::from_sgf(Color::White, "", size).unwrap(),
            Move::Pass(Color::White)
        );
    }
    assert_eq!(
        Move::from_sgf(Color::Black, "tt", BoardSize::new(9, 19).unwrap()).unwrap(),
        Move::Pass(Color::Black)
    );
}

#[test]
fn malformed_coordinates_and_off_board_points_are_rejected() {
    let size = BoardSize::new(19, 19).unwrap();
    for value in [
        "", " ", "A", "1", "A0", "A00", "A01", "A20", "U1", "Z25", "AA1", "A-1", "A+1", "A100",
        "A1 ", " A1", "PAss ", "é1", "中", "A\0", "Ａ1",
    ] {
        assert!(
            Point::from_gtp(value, size).is_err(),
            "accepted GTP {value:?}"
        );
        assert!(
            Move::from_gtp(Color::Black, value, size).is_err(),
            "accepted GTP {value:?}"
        );
    }
    for value in [
        "a", "aaa", "Aa", "aA", "zz", "ta", "at", "a ", " aa", "aa ", "é", "中", "\0a",
    ] {
        assert!(
            Point::from_sgf(value, size).is_err(),
            "accepted SGF {value:?}"
        );
        assert!(
            Move::from_sgf(Color::Black, value, size).is_err(),
            "accepted SGF {value:?}"
        );
    }
    for point in [
        Point { x: 19, y: 0 },
        Point { x: 0, y: 19 },
        Point { x: 255, y: 255 },
    ] {
        let expected = Err(CoordinateError::OutOfBounds { point, size });
        assert_eq!(point.to_gtp(size), expected);
        assert_eq!(point.to_sgf(size), expected);
        assert_eq!(Move::Play(Color::Black, point).to_gtp(size), expected);
        assert_eq!(Move::Play(Color::Black, point).to_sgf(size), expected);
    }
}

#[test]
fn every_conversion_validates_public_board_dimensions_including_pass() {
    for size in [
        BoardSize {
            width: 0,
            height: 19,
        },
        BoardSize {
            width: 19,
            height: 26,
        },
    ] {
        let point = Point { x: 0, y: 0 };
        let error = CoordinateError::InvalidBoardSize(BoardSizeError {
            width: size.width,
            height: size.height,
        });
        assert_eq!(point.to_gtp(size), Err(error.clone()));
        assert_eq!(point.to_sgf(size), Err(error.clone()));
        for mv in [Move::Play(Color::Black, point), Move::Pass(Color::Black)] {
            assert_eq!(mv.to_gtp(size), Err(error.clone()));
            assert_eq!(mv.to_sgf(size), Err(error.clone()));
        }
        for value in ["A1", "pass"] {
            assert_eq!(Point::from_gtp(value, size), Err(error.clone()));
            assert_eq!(
                Move::from_gtp(Color::Black, value, size),
                Err(error.clone())
            );
        }
        for value in ["aa", "", "tt"] {
            assert_eq!(Point::from_sgf(value, size), Err(error.clone()));
            assert_eq!(
                Move::from_sgf(Color::Black, value, size),
                Err(error.clone())
            );
        }
    }
}

proptest! {
    #[test]
    fn arbitrary_board_coordinates_round_trip(
        width in 2_u8..=25,
        height in 2_u8..=25,
        x in any::<u8>(),
        y in any::<u8>(),
    ) {
        let size = BoardSize::new(width, height).unwrap();
        let point = Point { x: x % width, y: y % height };
        prop_assert_eq!(Point::from_gtp(&point.to_gtp(size).unwrap(), size).unwrap(), point);
        prop_assert_eq!(Point::from_sgf(&point.to_sgf(size).unwrap(), size).unwrap(), point);
    }

    #[test]
    fn arbitrary_input_never_panics_and_valid_coordinates_canonicalize(value in any::<String>()) {
        let size = BoardSize::new(19, 19).unwrap();
        if let Ok(mv) = Move::from_gtp(Color::Black, &value, size) {
            prop_assert_eq!(Move::from_gtp(Color::Black, &mv.to_gtp(size).unwrap(), size).unwrap(), mv);
        }
        if let Ok(mv) = Move::from_sgf(Color::White, &value, size) {
            prop_assert_eq!(Move::from_sgf(Color::White, &mv.to_sgf(size).unwrap(), size).unwrap(), mv);
        }
    }
}
