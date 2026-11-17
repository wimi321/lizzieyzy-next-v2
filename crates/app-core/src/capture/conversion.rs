use super::*;
use board_capture as core;

impl From<CapturePoint> for core::PixelPoint {
    fn from(value: CapturePoint) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

impl From<core::PixelPoint> for CapturePoint {
    fn from(value: core::PixelPoint) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

impl From<CaptureSettings> for core::Thresholds {
    fn from(value: CaptureSettings) -> Self {
        Self {
            black_offset: value.black_offset,
            black_percent: value.black_percent,
            white_offset: value.white_offset,
            white_percent: value.white_percent,
            gray_offset: value.gray_offset,
        }
    }
}

impl From<CaptureCorner> for core::Corner {
    fn from(value: CaptureCorner) -> Self {
        match value {
            CaptureCorner::TopLeft => Self::TopLeft,
            CaptureCorner::TopRight => Self::TopRight,
            CaptureCorner::BottomLeft => Self::BottomLeft,
            CaptureCorner::BottomRight => Self::BottomRight,
        }
    }
}

impl From<core::Corner> for CaptureCorner {
    fn from(value: core::Corner) -> Self {
        match value {
            core::Corner::TopLeft => Self::TopLeft,
            core::Corner::TopRight => Self::TopRight,
            core::Corner::BottomLeft => Self::BottomLeft,
            core::Corner::BottomRight => Self::BottomRight,
        }
    }
}

impl From<CaptureCells> for core::Cells {
    fn from(value: CaptureCells) -> Self {
        Self {
            rows: value.rows,
            columns: value.columns,
            cells: value
                .cells
                .into_iter()
                .map(|cell| match cell {
                    CaptureCell::Empty => core::Cell::Empty,
                    CaptureCell::Black => core::Cell::Black,
                    CaptureCell::White => core::Cell::White,
                })
                .collect(),
        }
    }
}

impl From<core::Cells> for CaptureCells {
    fn from(value: core::Cells) -> Self {
        Self {
            rows: value.rows,
            columns: value.columns,
            cells: value
                .cells
                .into_iter()
                .map(|cell| match cell {
                    core::Cell::Empty => CaptureCell::Empty,
                    core::Cell::Black => CaptureCell::Black,
                    core::Cell::White => CaptureCell::White,
                })
                .collect(),
        }
    }
}

impl From<core::Grid> for CaptureGrid {
    fn from(value: core::Grid) -> Self {
        Self {
            origin: value.origin.into(),
            gap_x: value.gap_x,
            gap_y: value.gap_y,
            columns: value.columns,
            rows: value.rows,
            corner: value.corner.into(),
        }
    }
}

impl From<core::CaptureError> for CaptureError {
    fn from(value: core::CaptureError) -> Self {
        match value {
            core::CaptureError::InvalidImage => Self::InvalidImage,
            core::CaptureError::DoesNotFit => Self::DoesNotFit,
            core::CaptureError::InvalidCells => Self::InvalidCells,
            core::CaptureError::InvalidBoardSize => Self::InvalidBoardSize,
            error => Self::GridFit {
                reason: error.to_string(),
            },
        }
    }
}

pub(super) fn recognize(
    image: &CaptureImage,
    points: [CapturePoint; 3],
    settings: CaptureSettings,
) -> Result<CaptureRecognition, CaptureError> {
    let grid = core::fit_grid(points.map(Into::into))?;
    let cells = core::recognize(
        core::RgbaImage {
            width: image.width,
            height: image.height,
            pixels: &image.rgba,
        },
        &grid,
        &settings.into(),
    )?;
    Ok(CaptureRecognition {
        grid: grid.into(),
        cells: cells.into(),
    })
}
