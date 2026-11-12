import type { PointView } from "@/api/backend";

/**
 * Layout in canvas pixels. The board's canvases have one pixel per device pixel, which is how
 * Java lays the board out and what keeps lines and stones on whole pixels at any display scale.
 */
export interface Geometry {
  /** Board intersections. */
  columns: number;
  rows: number;
  /** Size of the canvas. */
  width: number;
  height: number;
  /** Canvas pixels per CSS pixel, for the few things that keep one size to the eye on every screen. */
  dpr: number;
  /** Distance between adjacent lines, a whole number of pixels as in Java. */
  cell: number;
  /** Center of point (0, 0), the top-left intersection. */
  originX: number;
  originY: number;
  /** Wood margin around the outer lines; wider when coordinates are shown. */
  margin: number;
  /** The wooden board: its top-left corner and size, as Java's `boardWidth`. */
  boardX: number;
  boardY: number;
  boardWidth: number;
  boardHeight: number;
  /** Java's `stoneRadius`: half a cell less one pixel. */
  stoneRadius: number;
  /** Java's `availableWidth`: from the first line to the last, inclusive. */
  available: number;
}

const BIG_MARGIN = 0.055;
const SMALL_MARGIN = 0.03;

/**
 * Java's `calculatePixelMargins` for one axis: shrink the board until the lines land on whole
 * pixels, `(available - 1)` a multiple of the line count less one.
 */
function axis(length: number, lines: number, ratio: number): { board: number; margin: number; available: number } {
  const marginRatio = (ratio / lines) * 19;
  for (let board = Math.floor(length); ; board--) {
    const margin = Math.trunc(marginRatio * board);
    const available = board - 2 * margin;
    if (board <= 0 || (available - 1) % (lines - 1) === 0) return { board, margin, available };
  }
}

/**
 * LizzieYzy Next's board metrics (`BoardRenderer.setupSizeParameters`): margins of 5.5% with
 * coordinates (3% without), whole-pixel line spacing, the board centred in the canvas.
 */
export function computeGeometry(
  columns: number,
  rows: number,
  canvasWidth: number,
  canvasHeight: number,
  dpr: number,
  bigMargin: boolean,
): Geometry {
  const ratio = bigMargin ? BIG_MARGIN : SMALL_MARGIN;
  const square = columns === rows;
  const horizontal = axis(square ? Math.min(canvasWidth, canvasHeight) : canvasWidth, columns, ratio);
  const vertical = square ? { ...horizontal } : axis(canvasHeight, rows, ratio);
  let cellX = Math.trunc(horizontal.available / Math.max(1, columns - 1));
  let cellY = Math.trunc(vertical.available / Math.max(1, rows - 1));
  if (cellX > cellY) {
    cellX = cellY;
    const width = cellX * (columns - 1) + 1;
    horizontal.board -= horizontal.available - width + (horizontal.margin - vertical.margin) * 2;
    horizontal.available = width;
    horizontal.margin = vertical.margin;
  } else if (cellX < cellY) {
    cellY = cellX;
    const height = cellY * (rows - 1) + 1;
    vertical.board -= vertical.available - height + (vertical.margin - horizontal.margin) * 2;
    vertical.available = height;
    vertical.margin = horizontal.margin;
  }
  const cell = Math.max(0, cellX);
  const boardX = Math.trunc((canvasWidth - horizontal.board) / 2);
  const boardY = Math.trunc((canvasHeight - vertical.board) / 2);
  return {
    columns,
    rows,
    width: canvasWidth,
    height: canvasHeight,
    dpr,
    cell,
    // Java's integer coordinate k is the pixel [k, k + 1); its center is k + 0.5.
    originX: boardX + horizontal.margin + 0.5,
    originY: boardY + vertical.margin + 0.5,
    margin: horizontal.margin,
    boardX,
    boardY,
    boardWidth: horizontal.board,
    boardHeight: vertical.board,
    stoneRadius: cell < 4 ? 1 : Math.trunc(cell / 2) - 1,
    available: horizontal.available,
  };
}

export function pointToPixel(geometry: Geometry, point: PointView): { x: number; y: number } {
  return {
    x: geometry.originX + point.x * geometry.cell,
    y: geometry.originY + point.y * geometry.cell,
  };
}

/** The intersection under a canvas-pixel position, or null outside half a cell. */
export function pixelToPoint(geometry: Geometry, x: number, y: number): PointView | null {
  if (geometry.cell <= 0) return null;
  // `|| 0` turns the -0 that rounding a tiny negative offset yields into 0.
  const column = Math.round((x - geometry.originX) / geometry.cell) || 0;
  const row = Math.round((y - geometry.originY) / geometry.cell) || 0;
  if (column < 0 || row < 0 || column >= geometry.columns || row >= geometry.rows) return null;
  const center = pointToPixel(geometry, { x: column, y: row });
  if (Math.abs(x - center.x) > geometry.cell / 2 || Math.abs(y - center.y) > geometry.cell / 2) {
    return null;
  }
  return { x: column, y: row };
}

/** Row-major index shared by BoardView.stones and AnalysisView.ownership. */
export function pointIndex(columns: number, point: PointView): number {
  return point.y * columns + point.x;
}

/** Star points: corners at the 3-3 or 4-4 line, plus center and sides on big boards. */
export function starPoints(columns: number, rows: number): PointView[] {
  const xs = starLines(columns);
  const ys = starLines(rows);
  const points: PointView[] = [];
  for (const y of ys) {
    for (const x of xs) {
      const cornerX = x === xs[0] || x === xs[xs.length - 1];
      const cornerY = y === ys[0] || y === ys[ys.length - 1];
      const middleX = !cornerX;
      const middleY = !cornerY;
      // Small boards mark the corners and the center only.
      const sides = columns >= 15 && rows >= 15;
      if ((cornerX && cornerY) || (middleX && middleY) || sides) points.push({ x, y });
    }
  }
  return points;
}

function starLines(size: number): number[] {
  if (size < 7) return [];
  const edge = size >= 13 ? 3 : 2;
  const lines = [edge, size - 1 - edge];
  if (size % 2 === 1) lines.splice(1, 0, (size - 1) / 2);
  return lines;
}
