use super::*;
use proptest::prelude::*;

const WOOD: [u8; 4] = [215, 162, 81, 255];
const PAPER: [u8; 4] = [255, 255, 255, 255];

fn point(x: f64, y: f64) -> PixelPoint {
    PixelPoint { x, y }
}

fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    rgba.repeat((width * height) as usize)
}

fn set_pixel(pixels: &mut [u8], width: u32, x: u32, y: u32, rgba: [u8; 4]) {
    let index = ((y * width + x) * 4) as usize;
    pixels[index..index + 4].copy_from_slice(&rgba);
}

fn one_cell(pixels: &[u8], width: u32, height: u32, settings: Thresholds) -> Cell {
    let grid = Grid {
        origin: point(f64::from(width) / 2.0, f64::from(height) / 2.0),
        gap_x: f64::from(width),
        gap_y: f64::from(height),
        rows: 1,
        columns: 1,
        corner: Corner::TopLeft,
    };
    recognize(
        RgbaImage {
            width,
            height,
            pixels,
        },
        &grid,
        &settings,
    )
    .unwrap()
    .cells[0]
}

#[test]
fn fits_four_directions_and_keeps_screen_order() {
    for (dx, dy, corner) in [
        (1.0, 1.0, Corner::TopLeft),
        (-1.0, 1.0, Corner::TopRight),
        (1.0, -1.0, Corner::BottomLeft),
        (-1.0, -1.0, Corner::BottomRight),
    ] {
        let grid = fit_grid([
            point(100.0, 100.0),
            point(100.0 + dx * 10.0, 100.0 + dy * 20.0),
            point(100.0 + dx * 42.0, 100.0 + dy * 63.0),
        ])
        .unwrap();
        assert_eq!((grid.columns, grid.rows), (5, 4));
        assert_eq!((grid.gap_x, grid.gap_y), (10.5, 21.0));
        assert_eq!(grid.corner, corner);
        assert_eq!(grid.origin.x, if dx > 0.0 { 100.0 } else { 58.0 });
        assert_eq!(grid.origin.y, if dy > 0.0 { 100.0 } else { 37.0 });
    }
}

#[test]
fn fitting_uses_nearest_integer_intervals_and_preserves_exact_gaps() {
    let grid = fit_grid([point(10.0, 20.0), point(20.0, 40.0), point(50.0, 80.0)]).unwrap();
    assert_eq!((grid.columns, grid.rows), (5, 4));
    assert_eq!((grid.gap_x, grid.gap_y), (10.0, 20.0));
    let halfway = fit_grid([point(0.0, 0.0), point(10.0, 10.0), point(25.0, 15.0)]).unwrap();
    assert_eq!((halfway.columns, halfway.rows), (4, 3));
    assert_eq!((halfway.gap_x, halfway.gap_y), (25.0 / 3.0, 7.5));
}

#[test]
fn one_row_column_and_intersection_keep_the_measured_sampling_size() {
    for (end, expected) in [
        (point(10.0, 60.0), (1, 3)),
        (point(40.0, 20.0), (4, 1)),
        (point(10.0, 20.0), (1, 1)),
    ] {
        let grid = fit_grid([point(10.0, 20.0), point(20.0, 40.0), end]).unwrap();
        assert_eq!((grid.columns, grid.rows), expected);
        assert_eq!((grid.gap_x, grid.gap_y), (10.0, 20.0));
    }
}

#[test]
fn invalid_clicks_and_unbounded_grids_are_typed_errors() {
    for second in [point(10.0, 30.0), point(20.0, 20.0), point(10.0, 20.0)] {
        assert_eq!(
            fit_grid([point(10.0, 20.0), second, point(30.0, 60.0)]),
            Err(CaptureError::ZeroGap)
        );
    }
    for bad in [
        point(f64::NAN, 1.0),
        point(1.0, f64::INFINITY),
        point(-1.0, 1.0),
    ] {
        assert_eq!(
            fit_grid([bad, point(20.0, 40.0), point(30.0, 60.0)]),
            Err(CaptureError::InvalidPoints)
        );
    }
    assert_eq!(
        fit_grid([point(10.0, 10.0), point(20.0, 20.0), point(5.0, 30.0)]),
        Err(CaptureError::InvalidPoints)
    );
    assert_eq!(
        fit_grid([point(10.0, 10.0), point(20.0, 20.0), point(11.0, 30.0)]),
        Err(CaptureError::InvalidGrid)
    );
    assert_eq!(
        fit_grid([point(0.0, 0.0), point(0.001, 0.001), point(100.0, 100.0)]),
        Err(CaptureError::GridTooLarge)
    );
}

#[test]
fn validates_buffer_before_sampling_and_rejects_invalid_public_grid() {
    for image in [
        RgbaImage {
            width: 0,
            height: 1,
            pixels: &[],
        },
        RgbaImage {
            width: 1,
            height: 1,
            pixels: &[0, 0, 0],
        },
        RgbaImage {
            width: u32::MAX,
            height: u32::MAX,
            pixels: &[],
        },
    ] {
        assert_eq!(image.validate(), Err(CaptureError::InvalidImage));
    }
    let bytes = solid(10, 10, PAPER);
    let image = RgbaImage {
        width: 10,
        height: 10,
        pixels: &bytes,
    };
    let grid = fit_grid([point(1.0, 1.0), point(2.0, 2.0), point(5.0, 5.0)]).unwrap();
    for changed in [
        Grid { columns: 0, ..grid },
        Grid {
            origin: point(-1.0, 0.0),
            ..grid
        },
        Grid {
            gap_x: f64::NAN,
            ..grid
        },
        Grid {
            gap_y: -1.0,
            ..grid
        },
        Grid { gap_x: 0.1, ..grid },
        Grid {
            gap_x: 11.0,
            columns: 1,
            ..grid
        },
        Grid {
            origin: point(10.0, 1.0),
            ..grid
        },
    ] {
        assert_eq!(
            recognize(image, &changed, &Thresholds::default()),
            Err(CaptureError::InvalidGrid)
        );
    }
    assert_eq!(
        recognize(
            image,
            &Grid {
                rows: u32::MAX,
                columns: u32::MAX,
                ..grid
            },
            &Thresholds::default()
        ),
        Err(CaptureError::GridTooLarge)
    );
}

#[test]
fn thresholds_include_endpoints_but_reject_every_out_of_range_field() {
    let defaults = Thresholds::default();
    assert_eq!(
        defaults,
        Thresholds {
            black_offset: 120,
            black_percent: 25,
            white_offset: 120,
            white_percent: 15,
            gray_offset: 80
        }
    );
    for settings in [
        Thresholds {
            black_offset: 0,
            white_offset: 0,
            gray_offset: 0,
            black_percent: 1,
            white_percent: 1,
        },
        Thresholds {
            black_offset: 255,
            white_offset: 255,
            gray_offset: 255,
            black_percent: 100,
            white_percent: 100,
        },
    ] {
        assert_eq!(settings.validate(), Ok(()));
    }
    for invalid in [
        Thresholds {
            black_offset: 256,
            ..defaults
        },
        Thresholds {
            white_offset: 256,
            ..defaults
        },
        Thresholds {
            gray_offset: 256,
            ..defaults
        },
        Thresholds {
            black_percent: 0,
            ..defaults
        },
        Thresholds {
            black_percent: 101,
            ..defaults
        },
        Thresholds {
            white_percent: 0,
            ..defaults
        },
        Thresholds {
            white_percent: 101,
            ..defaults
        },
    ] {
        assert_eq!(invalid.validate(), Err(CaptureError::InvalidThresholds));
    }
}

#[test]
fn black_threshold_is_inclusive_and_counts_all_three_channels() {
    let mut pixels = solid(10, 10, WOOD);
    for pixel in pixels.chunks_exact_mut(4).take(25) {
        pixel.copy_from_slice(&[120, 120, 120, 255]);
    }
    assert_eq!(
        one_cell(&pixels, 10, 10, Thresholds::default()),
        Cell::Black
    );
    pixels[0] = 121;
    assert_eq!(
        one_cell(&pixels, 10, 10, Thresholds::default()),
        Cell::Empty
    );
}

#[test]
fn percentages_are_truncated_integers_over_the_whole_rectangle() {
    let mut pixels = solid(6, 6, WOOD);
    for pixel in pixels.chunks_exact_mut(4).take(9) {
        pixel.copy_from_slice(&[0, 0, 0, 255]);
    }
    assert_eq!(one_cell(&pixels, 6, 6, Thresholds::default()), Cell::Black);
    pixels[0..4].copy_from_slice(&WOOD);
    assert_eq!(one_cell(&pixels, 6, 6, Thresholds::default()), Cell::Empty);
    // 8 / 36 is 22%, not rounded to 23%; colored pixels stay in the denominator.
    assert_eq!(
        one_cell(
            &pixels,
            6,
            6,
            Thresholds {
                black_percent: 23,
                ..Thresholds::default()
            }
        ),
        Cell::Empty
    );
    assert_eq!(
        one_cell(
            &pixels,
            6,
            6,
            Thresholds {
                black_percent: 22,
                ..Thresholds::default()
            }
        ),
        Cell::Black
    );
}

#[test]
fn black_has_priority_when_black_and_white_both_qualify() {
    let pixels = solid(10, 10, [100, 100, 100, 255]);
    assert_eq!(
        one_cell(
            &pixels,
            10,
            10,
            Thresholds {
                white_offset: 200,
                ..Thresholds::default()
            }
        ),
        Cell::Black
    );
    let mut mixed = solid(10, 10, PAPER);
    for pixel in mixed.chunks_exact_mut(4).take(25) {
        pixel.copy_from_slice(&[0, 0, 0, 255]);
    }
    assert_eq!(one_cell(&mixed, 10, 10, Thresholds::default()), Cell::Black);
}

#[test]
fn gray_filter_is_strict_and_checks_each_channel_pair() {
    for rgba in [[120, 40, 40, 255], [40, 120, 40, 255], [40, 40, 120, 255]] {
        let pixels = solid(10, 10, rgba);
        assert_eq!(
            one_cell(&pixels, 10, 10, Thresholds::default()),
            Cell::Empty
        );
        assert_eq!(
            one_cell(
                &pixels,
                10,
                10,
                Thresholds {
                    gray_offset: 81,
                    ..Thresholds::default()
                }
            ),
            Cell::Black
        );
    }
    // Even exact grayscale fails a strictly-less-than-zero gray tolerance.
    assert_eq!(
        one_cell(
            &solid(10, 10, [0, 0, 0, 255]),
            10,
            10,
            Thresholds {
                gray_offset: 0,
                ..Thresholds::default()
            }
        ),
        Cell::Empty
    );
    // A colored bright background does not count as white either.
    assert_eq!(
        one_cell(
            &solid(10, 10, [255, 135, 135, 255]),
            10,
            10,
            Thresholds::default()
        ),
        Cell::Empty
    );
}

#[test]
fn white_offset_and_minimum_percentage_are_inclusive() {
    let mut pixels = solid(10, 10, WOOD);
    for pixel in pixels.chunks_exact_mut(4).take(15) {
        pixel.copy_from_slice(&[135, 135, 135, 255]);
    }
    assert_eq!(
        one_cell(&pixels, 10, 10, Thresholds::default()),
        Cell::White
    );
    pixels[0] = 134;
    assert_eq!(
        one_cell(&pixels, 10, 10, Thresholds::default()),
        Cell::Empty
    );
}

#[test]
fn white_paper_without_stones_is_empty() {
    assert_eq!(
        one_cell(&solid(10, 10, PAPER), 10, 10, Thresholds::default()),
        Cell::Empty
    );
    assert_eq!(
        one_cell(
            &solid(10, 10, [225, 225, 225, 255]),
            10,
            10,
            Thresholds::default()
        ),
        Cell::Empty
    );
}

#[test]
fn true_white_uses_rounded_quarter_row_and_exclusive_rounded_third_width() {
    for channel in 0..3 {
        let mut pixels = solid(10, 10, PAPER);
        // Java rounds 10/4 to 3 and 10*2/6 to 3 (x = 0,1,2).
        let mut dot = PAPER;
        dot[channel] = 224;
        set_pixel(&mut pixels, 10, 2, 3, dot);
        assert_eq!(
            one_cell(&pixels, 10, 10, Thresholds::default()),
            Cell::White
        );
    }
    for (x, y) in [(3, 3), (2, 2), (2, 4)] {
        let mut pixels = solid(10, 10, PAPER);
        set_pixel(&mut pixels, 10, x, y, [224, 255, 255, 255]);
        assert_eq!(
            one_cell(&pixels, 10, 10, Thresholds::default()),
            Cell::Empty
        );
    }
    // trueWhite is not subject to the gray filter used for the percentages.
    let mut pixels = solid(10, 10, PAPER);
    set_pixel(&mut pixels, 10, 0, 3, [0, 255, 255, 255]);
    assert_eq!(
        one_cell(&pixels, 10, 10, Thresholds::default()),
        Cell::White
    );
}

#[test]
fn near_white_minus_pure_white_distinguishes_shaded_stones_on_paper() {
    for (count, gray, expected) in [
        (10, 190, Cell::White),
        (9, 190, Cell::Empty),
        (10, 189, Cell::Empty),
        (10, 224, Cell::White),
        (10, 225, Cell::Empty),
    ] {
        let mut pixels = solid(10, 10, PAPER);
        for pixel in pixels.chunks_exact_mut(4).rev().take(count) {
            pixel.copy_from_slice(&[gray, gray, gray, 255]);
        }
        assert_eq!(
            one_cell(&pixels, 10, 10, Thresholds::default()),
            expected,
            "count={count}, gray={gray}"
        );
    }
}

#[test]
fn sampling_moves_the_whole_rectangle_inside_each_image_edge() {
    let mut pixels = solid(8, 8, WOOD);
    // Four independent 4 x 4 quadrants, with centers at the extreme image corners.
    for y in 0..4 {
        for x in 0..4 {
            set_pixel(&mut pixels, 8, x, y, [0, 0, 0, 255]);
            set_pixel(&mut pixels, 8, x + 4, y + 4, [0, 0, 0, 255]);
        }
    }
    for (origin, expected) in [
        (point(0.0, 0.0), Cell::Black),
        (point(7.0, 0.0), Cell::Empty),
        (point(0.0, 7.0), Cell::Empty),
        (point(7.0, 7.0), Cell::Black),
    ] {
        let grid = Grid {
            origin,
            gap_x: 4.0,
            gap_y: 4.0,
            columns: 1,
            rows: 1,
            corner: Corner::TopLeft,
        };
        assert_eq!(
            recognize(
                RgbaImage {
                    width: 8,
                    height: 8,
                    pixels: &pixels
                },
                &grid,
                &Thresholds::default()
            )
            .unwrap()
            .cells,
            vec![expected]
        );
    }
}

#[test]
fn place_preserves_row_order_at_all_four_corners() {
    let cells = Cells {
        rows: 2,
        columns: 3,
        cells: vec![
            Cell::Black,
            Cell::Empty,
            Cell::White,
            Cell::White,
            Cell::Black,
            Cell::Empty,
        ],
    };
    for (corner, x, y) in [
        (Corner::TopLeft, 0, 0),
        (Corner::TopRight, 6, 0),
        (Corner::BottomLeft, 0, 7),
        (Corner::BottomRight, 6, 7),
    ] {
        let board = place(&cells, corner, 9).unwrap();
        assert_eq!(board.len(), 81);
        assert_eq!(board[y * 9 + x], Some(Color::Black));
        assert_eq!(board[y * 9 + x + 2], Some(Color::White));
        assert_eq!(board[(y + 1) * 9 + x], Some(Color::White));
        assert_eq!(board[(y + 1) * 9 + x + 1], Some(Color::Black));
        assert_eq!(board.iter().filter(|stone| stone.is_some()).count(), 4);
    }
}

#[test]
fn placement_rejects_oversize_and_malformed_cells_without_partial_results() {
    let cells = Cells {
        rows: 2,
        columns: 3,
        cells: vec![Cell::Empty; 6],
    };
    assert_eq!(
        place(&cells, Corner::TopLeft, 2),
        Err(CaptureError::DoesNotFit)
    );
    for size in [0, 1, 26, 255] {
        assert_eq!(
            place(&cells, Corner::TopLeft, size),
            Err(CaptureError::InvalidBoardSize)
        );
    }
    for invalid in [
        Cells {
            rows: 0,
            columns: 0,
            cells: vec![],
        },
        Cells {
            rows: 1,
            columns: 0,
            cells: vec![],
        },
        Cells {
            rows: 2,
            columns: 3,
            cells: vec![Cell::Black; 5],
        },
        Cells {
            rows: u32::MAX,
            columns: u32::MAX,
            cells: vec![],
        },
    ] {
        assert_eq!(
            place(&invalid, Corner::TopLeft, 19),
            Err(CaptureError::InvalidCells)
        );
    }
}

#[path = "tests_golden.rs"]
mod golden;
#[path = "tests_synthetic.rs"]
mod synthetic;
