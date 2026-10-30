import type { PointView } from "@/api/backend";

/** Layout in CSS pixels. Canvas backing stores multiply by `dpr`. */
export interface Geometry {
  /** Board intersections. */
  columns: number;
  rows: number;
  /** CSS size of the canvas. */
  cssWidth: number;
  cssHeight: number;
  dpr: number;
  /** Distance between adjacent lines. */
  cell: number;
  /** Center of point (0, 0), the top-left intersection. */
  originX: number;
  originY: number;
  /** Wood margin around the outer lines; wider when coordinates are shown. */
  margin: number;
}

const MARGIN_CELLS = 0.7;
const MARGIN_CELLS_WITH_COORDINATES = 1.35;

export function computeGeometry(
  columns: number,
  rows: number,
  cssWidth: number,
  cssHeight: number,
  dpr: number,
  coordinates: boolean,
): Geometry {
  const marginCells = coordinates ? MARGIN_CELLS_WITH_COORDINATES : MARGIN_CELLS;
  const spanX = columns - 1 + 2 * marginCells;
  const spanY = rows - 1 + 2 * marginCells;
  const cell = Math.max(0, Math.min(cssWidth / spanX, cssHeight / spanY));
  return {
    columns,
    rows,
    cssWidth,
    cssHeight,
    dpr,
    cell,
    originX: (cssWidth - cell * (columns - 1)) / 2,
    originY: (cssHeight - cell * (rows - 1)) / 2,
    margin: cell * marginCells,
  };
}

/** Backing-store size for crisp lines on high-DPI screens. */
export function backingSize(geometry: Geometry): { width: number; height: number } {
  return {
    width: Math.max(1, Math.round(geometry.cssWidth * geometry.dpr)),
    height: Math.max(1, Math.round(geometry.cssHeight * geometry.dpr)),
  };
}

export function pointToPixel(geometry: Geometry, point: PointView): { x: number; y: number } {
  return {
    x: geometry.originX + point.x * geometry.cell,
    y: geometry.originY + point.y * geometry.cell,
  };
}

/** The intersection under a CSS-pixel position, or null outside half a cell. */
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
