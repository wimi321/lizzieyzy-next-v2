#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn size_accepts_rectangles_and_inclusive_bounds() {
    for width in 2..=25 {
        for height in 2..=25 {
            let size = BoardSize::new(width, height).unwrap();
            assert_eq!(size.area(), usize::from(width) * usize::from(height));
            assert!(size.contains(Point {
                x: width - 1,
                y: height - 1,
            }));
            assert!(!size.contains(Point { x: width, y: 0 }));
            assert!(!size.contains(Point { x: 0, y: height }));
        }
    }
}

#[test]
fn size_rejects_invalid_dimensions_even_from_public_fields() {
    for invalid in [0, 1, 26, 255] {
        for (width, height) in [(invalid, 19), (19, invalid)] {
            let error = BoardSize::new(width, height).unwrap_err();
            assert_eq!(error, BoardSizeError { width, height });
            assert_eq!(BoardSize { width, height }.validate(), Err(error));
            assert!(error.to_string().contains("2..=25"));
        }
    }
}

#[test]
fn moves_preserve_color_and_opponents_alternate() {
    for color in [Color::Black, Color::White] {
        assert_ne!(color.opponent(), color);
        assert_eq!(color.opponent().opponent(), color);
        assert_eq!(Move::Pass(color).color(), color);
        assert_eq!(Move::Play(color, Point { x: 0, y: 0 }).color(), color);
    }
}
