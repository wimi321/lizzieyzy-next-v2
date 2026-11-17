use super::*;

/// Generated pixels only: wood or white paper, grid lines, shaded disks and
/// optional contrasting 3x5 bitmap move numbers. No renderer or screen capture.
fn synthetic(cells: &Cells, paper: bool, numbers: bool) -> (u32, u32, Vec<u8>) {
    let (gap_x, gap_y) = (24, 28);
    let (width, height) = (cells.columns * gap_x, cells.rows * gap_y);
    let mut pixels = solid(width, height, if paper { PAPER } else { WOOD });
    for y in 0..height {
        for x in 0..width {
            if x % gap_x == gap_x / 2 || y % gap_y == gap_y / 2 {
                set_pixel(&mut pixels, width, x, y, [65, 65, 65, 255]);
            }
        }
    }
    for (index, cell) in cells.cells.iter().enumerate() {
        if *cell == Cell::Empty {
            continue;
        }
        let center_x = index as u32 % cells.columns * gap_x + gap_x / 2;
        let center_y = index as u32 / cells.columns * gap_y + gap_y / 2;
        for y in center_y - 12..=center_y + 12 {
            for x in center_x - 10..=center_x + 10 {
                let dx = (f64::from(x) - f64::from(center_x)) / 10.0;
                let dy = (f64::from(y) - f64::from(center_y)) / 12.0;
                let radius = dx * dx + dy * dy;
                if radius <= 1.0 {
                    let color = match cell {
                        Cell::Black => [20, 20, 20, 255],
                        Cell::White if radius > 0.55 => [205, 205, 205, 255],
                        _ => [245, 245, 245, 255],
                    };
                    set_pixel(&mut pixels, width, x, y, color);
                }
            }
        }
        if numbers {
            let color = if *cell == Cell::Black {
                PAPER
            } else {
                [0, 0, 0, 255]
            };
            let digit = if index % 2 == 0 {
                [2_u8, 6, 2, 2, 7]
            } else {
                [7, 1, 7, 4, 7]
            };
            for (row, mask) in digit.iter().enumerate() {
                for column in 0..3 {
                    if mask & (1 << (2 - column)) != 0 {
                        set_pixel(
                            &mut pixels,
                            width,
                            center_x - 1 + column,
                            center_y - 2 + row as u32,
                            color,
                        );
                    }
                }
            }
        }
    }
    (width, height, pixels)
}

fn selected_grid(cells: &Cells, corner: Corner) -> Grid {
    let min = point(12.0, 14.0);
    let max = point(
        min.x + f64::from(cells.columns - 1) * 24.0,
        min.y + f64::from(cells.rows - 1) * 28.0,
    );
    let (first, last, dx, dy) = match corner {
        Corner::TopLeft => (min, max, 24.0, 28.0),
        Corner::TopRight => (point(max.x, min.y), point(min.x, max.y), -24.0, 28.0),
        Corner::BottomLeft => (point(min.x, max.y), point(max.x, min.y), 24.0, -28.0),
        Corner::BottomRight => (max, min, -24.0, -28.0),
    };
    // Keep point 2 nonnegative for single row/column fixtures in lower/right corners.
    // The measured spacing itself is independent of where the eventual rectangle ends.
    let offset_x = if first.x + dx < 0.0 { 24.0 } else { 0.0 };
    let offset_y = if first.y + dy < 0.0 { 28.0 } else { 0.0 };
    let shifted = |p: PixelPoint| point(p.x + offset_x, p.y + offset_y);
    let mut grid = fit_grid([
        shifted(first),
        shifted(point(first.x + dx, first.y + dy)),
        shifted(last),
    ])
    .unwrap();
    grid.origin.x -= offset_x;
    grid.origin.y -= offset_y;
    grid
}

#[test]
fn synthetic_wood_paper_rectangles_and_numbered_stones_match_layout() {
    let cells = Cells {
        rows: 4,
        columns: 5,
        cells: vec![
            Cell::Black,
            Cell::White,
            Cell::Empty,
            Cell::Black,
            Cell::Empty,
            Cell::Empty,
            Cell::Black,
            Cell::White,
            Cell::White,
            Cell::Black,
            Cell::White,
            Cell::Empty,
            Cell::Empty,
            Cell::Black,
            Cell::White,
            Cell::Black,
            Cell::White,
            Cell::Black,
            Cell::Empty,
            Cell::Empty,
        ],
    };
    for paper in [false, true] {
        for numbers in [false, true] {
            let (width, height, pixels) = synthetic(&cells, paper, numbers);
            for corner in [
                Corner::TopLeft,
                Corner::TopRight,
                Corner::BottomLeft,
                Corner::BottomRight,
            ] {
                let grid = selected_grid(&cells, corner);
                let actual = recognize(
                    RgbaImage {
                        width,
                        height,
                        pixels: &pixels,
                    },
                    &grid,
                    &Thresholds::default(),
                )
                .unwrap();
                assert_eq!(
                    actual, cells,
                    "paper={paper}, numbers={numbers}, corner={corner:?}"
                );
                assert_eq!(
                    place(&actual, corner, 9).unwrap(),
                    place(&cells, corner, 9).unwrap()
                );
            }
        }
    }
}

#[test]
fn noninteger_spacing_samples_each_center_without_accumulated_rounding() {
    let grid = fit_grid([point(5.6, 5.0), point(15.6, 15.0), point(26.4, 5.0)]).unwrap();
    assert_eq!((grid.columns, grid.rows), (3, 1));
    assert!((grid.gap_x - 10.4).abs() < 1e-10);
    let mut pixels = solid(32, 10, WOOD);
    // The middle center is x=16; its rectangle starts at round(16-10.4/2)=11.
    // Round the complete coordinate, rather than separately rounding the origin
    // and column offset (which would start at 10 and miss the last dark column).
    for y in 0..10 {
        for x in 18..21 {
            set_pixel(&mut pixels, 32, x, y, [0, 0, 0, 255]);
        }
    }
    let result = recognize(
        RgbaImage {
            width: 32,
            height: 10,
            pixels: &pixels,
        },
        &grid,
        &Thresholds::default(),
    )
    .unwrap();
    assert_eq!(result.cells, vec![Cell::Empty, Cell::Black, Cell::Empty]);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn arbitrary_layouts_on_generated_boards_round_trip(
        (rows, columns, layout) in (1_u32..7, 1_u32..7).prop_flat_map(|(rows, columns)| {
            (Just(rows), Just(columns), prop::collection::vec(0_u8..3, (rows * columns) as usize))
        }),
        paper in any::<bool>(),
        numbers in any::<bool>(),
        corner_index in 0_usize..4,
    ) {
        let cells = Cells { rows, columns, cells: layout.into_iter().map(|n| match n { 0 => Cell::Empty, 1 => Cell::Black, _ => Cell::White }).collect() };
        let corner = [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight][corner_index];
        let grid = selected_grid(&cells, corner);
        let (width, height, pixels) = synthetic(&cells, paper, numbers);
        let result = recognize(RgbaImage { width, height, pixels: &pixels }, &grid, &Thresholds::default()).unwrap();
        prop_assert_eq!(&result, &cells);
        let board = place(&result, corner, 9).unwrap();
        let offset_x = if matches!(corner, Corner::TopRight | Corner::BottomRight) { 9 - columns } else { 0 };
        let offset_y = if matches!(corner, Corner::BottomLeft | Corner::BottomRight) { 9 - rows } else { 0 };
        for (index, cell) in cells.cells.iter().enumerate() {
            let expected = match cell { Cell::Empty => None, Cell::Black => Some(Color::Black), Cell::White => Some(Color::White) };
            let x = index as u32 % columns + offset_x;
            let y = index as u32 / columns + offset_y;
            prop_assert_eq!(board[(y * 9 + x) as usize], expected);
        }
        prop_assert_eq!(board.iter().filter(|s| s.is_some()).count(), cells.cells.iter().filter(|s| **s != Cell::Empty).count());
    }

    #[test]
    fn arbitrary_pixels_and_small_grids_never_panic(width in 1_u32..30, height in 1_u32..30, gap_x in 0.0_f64..40.0, gap_y in 0.0_f64..40.0, gray_offset in 0_u16..300) {
        let pixels = solid(width, height, [120, 200, 40, 255]);
        let grid = Grid { origin: point(0.0, 0.0), gap_x, gap_y, rows: 2, columns: 2, corner: Corner::BottomRight };
        let _ = recognize(RgbaImage { width, height, pixels: &pixels }, &grid, &Thresholds { gray_offset, ..Thresholds::default() });
    }
}
