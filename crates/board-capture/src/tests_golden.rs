use super::*;
use serde_json::Value;

#[test]
fn repository_png_crops_match_independently_recorded_stones_and_placement() {
    for (png, expected) in [
        (
            include_bytes!("../../../fixtures/board-capture/wood-top-left.png").as_slice(),
            include_str!("../../../fixtures/board-capture/wood-top-left.json"),
        ),
        (
            include_bytes!("../../../fixtures/board-capture/numbered-top-left.png").as_slice(),
            include_str!("../../../fixtures/board-capture/numbered-top-left.json"),
        ),
        (
            include_bytes!("../../../fixtures/board-capture/nine-bottom-right.png").as_slice(),
            include_str!("../../../fixtures/board-capture/nine-bottom-right.json"),
        ),
    ] {
        let expected: Value = serde_json::from_str(expected).unwrap();
        let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .unwrap()
            .into_rgba8();
        let points: [PixelPoint; 3] = std::array::from_fn(|i| {
            point(
                expected["points"][i][0].as_f64().unwrap(),
                expected["points"][i][1].as_f64().unwrap(),
            )
        });
        let grid = fit_grid(points).unwrap();
        let corner = match expected["corner"].as_str().unwrap() {
            "TopLeft" => Corner::TopLeft,
            "BottomRight" => Corner::BottomRight,
            name => panic!("unknown fixture corner {name}"),
        };
        assert_eq!(grid.corner, corner);
        let cells = Cells {
            rows: expected["rows"].as_u64().unwrap() as u32,
            columns: expected["columns"].as_u64().unwrap() as u32,
            cells: expected["cells"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|row| {
                    row.as_str().unwrap().chars().map(|c| match c {
                        'B' => Cell::Black,
                        'W' => Cell::White,
                        '.' => Cell::Empty,
                        _ => panic!("unknown fixture cell {c}"),
                    })
                })
                .collect(),
        };
        assert_eq!((grid.columns, grid.rows), (cells.columns, cells.rows));
        let actual = recognize(
            RgbaImage {
                width: image.width(),
                height: image.height(),
                pixels: image.as_raw(),
            },
            &grid,
            &Thresholds::default(),
        )
        .unwrap();
        assert_eq!(actual, cells, "{}", expected["source"]);
        assert_eq!(
            place(&actual, corner, 19).unwrap(),
            place(&cells, corner, 19).unwrap()
        );
    }
}
