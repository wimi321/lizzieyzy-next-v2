//! Pure, synchronous recognition of a manually selected Go grid in RGBA pixels.
//!
//! Sampling thresholds and white-paper rejection follow CaptureTsumeGo 1.2's
//! `recognizeBoard` / `getColorPercent`. There is no image decoding or I/O here.

use go_core::Color;
use thiserror::Error;

/// At most one million preview intersections, independent of the target board.
const MAX_CELLS: u64 = 1_000_000;

#[derive(Clone, Copy, Debug)]
pub struct RgbaImage<'a> {
    pub width: u32,
    pub height: u32,
    /// Tightly packed row-major RGBA8. Alpha is ignored, as in Java getRGB.
    pub pixels: &'a [u8],
}

impl RgbaImage<'_> {
    pub fn validate(&self) -> Result<(), CaptureError> {
        let expected = u64::from(self.width)
            .checked_mul(u64::from(self.height))
            .and_then(|area| area.checked_mul(4));
        if self.width == 0 || self.height == 0 || expected != Some(self.pixels.len() as u64) {
            return Err(CaptureError::InvalidImage);
        }
        Ok(())
    }

    fn rgb(&self, x: u32, y: u32) -> [u8; 3] {
        let index = (u64::from(y) * u64::from(self.width) + u64::from(x)) as usize * 4;
        [
            self.pixels[index],
            self.pixels[index + 1],
            self.pixels[index + 2],
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Black,
    White,
}

/// Screen order: from left to right, then top to bottom, regardless of corner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cells {
    pub rows: u32,
    pub columns: u32,
    pub cells: Vec<Cell>,
}

impl Cells {
    pub fn validate(&self) -> Result<(), CaptureError> {
        if self.rows == 0
            || self.columns == 0
            || u64::from(self.rows) * u64::from(self.columns) != self.cells.len() as u64
        {
            return Err(CaptureError::InvalidCells);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    /// The upper-left intersection in image pixels (not necessarily point 1).
    pub origin: PixelPoint,
    pub gap_x: f64,
    pub gap_y: f64,
    pub columns: u32,
    pub rows: u32,
    pub corner: Corner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thresholds {
    pub black_offset: u16,
    pub black_percent: u16,
    pub white_offset: u16,
    pub white_percent: u16,
    pub gray_offset: u16,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            black_offset: 120,
            black_percent: 25,
            white_offset: 120,
            white_percent: 15,
            gray_offset: 80,
        }
    }
}

impl Thresholds {
    pub fn validate(&self) -> Result<(), CaptureError> {
        if self.black_offset > 255
            || self.white_offset > 255
            || self.gray_offset > 255
            || !(1..=100).contains(&self.black_percent)
            || !(1..=100).contains(&self.white_percent)
        {
            return Err(CaptureError::InvalidThresholds);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum CaptureError {
    #[error("RGBA dimensions do not match the pixel buffer")]
    InvalidImage,
    #[error("capture thresholds are outside their allowed ranges")]
    InvalidThresholds,
    #[error("capture points must be finite, nonnegative and in the selected direction")]
    InvalidPoints,
    #[error("the first two intersections must differ in both coordinates")]
    ZeroGap,
    #[error("the selected grid contains too many intersections")]
    GridTooLarge,
    #[error("the grid cannot be sampled from this image")]
    InvalidGrid,
    #[error("capture cells do not fit the selected board")]
    DoesNotFit,
    #[error("capture cells must be a nonempty complete rectangle")]
    InvalidCells,
    #[error("the board size must be between 2 and 25")]
    InvalidBoardSize,
}

/// Fit each axis independently; the second click determines the board corner.
/// A one-row/column selection retains the first measured gap on that axis.
pub fn fit_grid(points: [PixelPoint; 3]) -> Result<Grid, CaptureError> {
    if points.iter().any(|point| !valid_point(*point)) {
        return Err(CaptureError::InvalidPoints);
    }
    let [first, second, last] = points;
    let dx = second.x - first.x;
    let dy = second.y - first.y;
    if dx == 0.0 || dy == 0.0 {
        return Err(CaptureError::ZeroGap);
    }
    let width = last.x - first.x;
    let height = last.y - first.y;
    if width * dx < 0.0 || height * dy < 0.0 {
        return Err(CaptureError::InvalidPoints);
    }
    let (columns, gap_x) = fit_axis(width.abs(), dx.abs())?;
    let (rows, gap_y) = fit_axis(height.abs(), dy.abs())?;
    if u64::from(rows) * u64::from(columns) > MAX_CELLS {
        return Err(CaptureError::GridTooLarge);
    }
    let corner = match (dx > 0.0, dy > 0.0) {
        (true, true) => Corner::TopLeft,
        (false, true) => Corner::TopRight,
        (true, false) => Corner::BottomLeft,
        (false, false) => Corner::BottomRight,
    };
    Ok(Grid {
        origin: PixelPoint {
            x: first.x.min(last.x),
            y: first.y.min(last.y),
        },
        gap_x,
        gap_y,
        columns,
        rows,
        corner,
    })
}

fn valid_point(point: PixelPoint) -> bool {
    [point.x, point.y]
        .iter()
        .all(|n| n.is_finite() && (0.0..=f64::from(u32::MAX)).contains(n))
}

fn fit_axis(span: f64, measured_gap: f64) -> Result<(u32, f64), CaptureError> {
    if span == 0.0 {
        return Ok((1, measured_gap));
    }
    let intervals = (span / measured_gap).round();
    if !intervals.is_finite() || intervals >= MAX_CELLS as f64 {
        return Err(CaptureError::GridTooLarge);
    }
    if intervals < 1.0 {
        return Err(CaptureError::InvalidGrid);
    }
    Ok((intervals as u32 + 1, span / intervals))
}

/// Recognize from raw RGBA. Sampling rectangles are rounded like Java's
/// Math.round and translated inside the image without changing their area.
pub fn recognize(
    image: RgbaImage<'_>,
    grid: &Grid,
    thresholds: &Thresholds,
) -> Result<Cells, CaptureError> {
    image.validate()?;
    thresholds.validate()?;
    let (width, height) = sample_dimensions(&image, grid)?;
    let mut cells = Vec::with_capacity((u64::from(grid.rows) * u64::from(grid.columns)) as usize);
    for row in 0..grid.rows {
        for column in 0..grid.columns {
            let x = grid.origin.x + f64::from(column) * grid.gap_x;
            let y = grid.origin.y + f64::from(row) * grid.gap_y;
            let left =
                java_round(x - grid.gap_x / 2.0).clamp(0.0, f64::from(image.width - width)) as u32;
            let top = java_round(y - grid.gap_y / 2.0).clamp(0.0, f64::from(image.height - height))
                as u32;
            cells.push(classify(&image, left, top, width, height, thresholds));
        }
    }
    Ok(Cells {
        rows: grid.rows,
        columns: grid.columns,
        cells,
    })
}

fn sample_dimensions(image: &RgbaImage<'_>, grid: &Grid) -> Result<(u32, u32), CaptureError> {
    if grid.columns == 0 || grid.rows == 0 {
        return Err(CaptureError::InvalidGrid);
    }
    if u64::from(grid.columns) * u64::from(grid.rows) > MAX_CELLS {
        return Err(CaptureError::GridTooLarge);
    }
    let last = PixelPoint {
        x: grid.origin.x + f64::from(grid.columns - 1) * grid.gap_x,
        y: grid.origin.y + f64::from(grid.rows - 1) * grid.gap_y,
    };
    if !valid_point(grid.origin)
        || !valid_point(last)
        || last.x >= f64::from(image.width)
        || last.y >= f64::from(image.height)
        || !grid.gap_x.is_finite()
        || !grid.gap_y.is_finite()
        || grid.gap_x < 0.5
        || grid.gap_y < 0.5
        || grid.gap_x.round() > f64::from(image.width)
        || grid.gap_y.round() > f64::from(image.height)
    {
        return Err(CaptureError::InvalidGrid);
    }
    Ok((grid.gap_x.round() as u32, grid.gap_y.round() as u32))
}

fn java_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

fn classify(
    image: &RgbaImage<'_>,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    thresholds: &Thresholds,
) -> Cell {
    let mut black = 0_u64;
    let mut white = 0_u64;
    let mut pure_white = 0_u64;
    let mut near_white = 0_u64;
    for y in top..top + height {
        for x in left..left + width {
            let [r, g, b] = image.rgb(x, y).map(u16::from);
            if r.abs_diff(g) < thresholds.gray_offset
                && g.abs_diff(b) < thresholds.gray_offset
                && b.abs_diff(r) < thresholds.gray_offset
            {
                black += u64::from([r, g, b].iter().all(|c| *c <= thresholds.black_offset));
                white += u64::from(
                    [r, g, b]
                        .iter()
                        .all(|c| *c >= 255 - thresholds.white_offset),
                );
                pure_white += u64::from([r, g, b].iter().all(|c| *c >= 225));
                near_white += u64::from([r, g, b].iter().all(|c| *c >= 190));
            }
        }
    }
    let area = u64::from(width) * u64::from(height);
    let percent = |count| 100 * count / area;
    if percent(black) >= u64::from(thresholds.black_percent) {
        return Cell::Black;
    }
    if percent(white) < u64::from(thresholds.white_percent) {
        return Cell::Empty;
    }
    // Deliberately no gray filter here: Java tests every RGB channel on this line.
    let sample_y = top + (f64::from(height) / 4.0).round() as u32;
    let sample_width = (f64::from(width) * 2.0 / 6.0).round() as u32;
    let true_white =
        (left..left + sample_width).any(|x| image.rgb(x, sample_y).iter().any(|c| *c < 225));
    if true_white || percent(near_white) - percent(pure_white) >= 10 {
        Cell::White
    } else {
        Cell::Empty
    }
}

/// Place in the selected corner, preserving screen row/column order. Empty
/// intersections elsewhere on the new square board are represented by None.
pub fn place(
    cells: &Cells,
    corner: Corner,
    board_size: u8,
) -> Result<Vec<Option<Color>>, CaptureError> {
    cells.validate()?;
    if !(2..=25).contains(&board_size) {
        return Err(CaptureError::InvalidBoardSize);
    }
    let size = u32::from(board_size);
    if cells.rows > size || cells.columns > size {
        return Err(CaptureError::DoesNotFit);
    }
    let offset_x = match corner {
        Corner::TopRight | Corner::BottomRight => size - cells.columns,
        Corner::TopLeft | Corner::BottomLeft => 0,
    };
    let offset_y = match corner {
        Corner::BottomLeft | Corner::BottomRight => size - cells.rows,
        Corner::TopLeft | Corner::TopRight => 0,
    };
    let mut board = vec![None; (size * size) as usize];
    for (index, cell) in cells.cells.iter().enumerate() {
        let x = index as u32 % cells.columns + offset_x;
        let y = index as u32 / cells.columns + offset_y;
        board[(y * size + x) as usize] = match cell {
            Cell::Empty => None,
            Cell::Black => Some(Color::Black),
            Cell::White => Some(Color::White),
        };
    }
    Ok(board)
}

#[cfg(test)]
mod tests;
