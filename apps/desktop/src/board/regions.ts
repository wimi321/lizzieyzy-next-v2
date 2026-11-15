import type { ColorView, PointView, RestrictionKind } from "@/api/backend";
import type { Geometry } from "./geometry";

/** Java's colours for 选点限制 (`BoardRenderer`): blue where the engine may play, red where it may not. */
const FILL: Record<RestrictionKind, string> = { Allow: "rgba(30, 90, 255, 0.32)", Avoid: "rgba(235, 40, 40, 0.32)" };
const EDGE: Record<RestrictionKind, string> = { Allow: "rgba(30, 90, 255, 0.9)", Avoid: "rgba(220, 30, 30, 0.9)" };

export type Pixel = readonly [x: number, y: number];

/** The lines a span of canvas pixels crosses on one axis, or null when it crosses none. */
function crossed(a: number, b: number, origin: number, cell: number, count: number): [number, number] | null {
  if (cell <= 0) return null;
  const first = Math.max(0, Math.ceil((Math.min(a, b) - origin) / cell));
  const last = Math.min(count - 1, Math.floor((Math.max(a, b) - origin) / cell));
  return first <= last ? [first, last] : null;
}

/** How far a rectangle must be dragged each way, in canvas pixels, not to be a click. */
const CLICK = 2;

/**
 * The intersections inside a rectangle dragged on the board, in canvas pixels. As in Java it
 * counts only when it crosses a line both ways: a click, even on a point, or a sliver between
 * two lines, is null.
 */
export function pointsInRect(geometry: Geometry, from: Pixel, to: Pixel): PointView[] | null {
  if (Math.abs(to[0] - from[0]) < CLICK || Math.abs(to[1] - from[1]) < CLICK) return null;
  const xs = crossed(from[0], to[0], geometry.originX, geometry.cell, geometry.columns);
  const ys = crossed(from[1], to[1], geometry.originY, geometry.cell, geometry.rows);
  if (!xs || !ys) return null;
  const points: PointView[] = [];
  for (let y = ys[0]; y <= ys[1]; y++) for (let x = xs[0]; x <= xs[1]; x++) points.push({ x, y });
  return points;
}

/** A square the size of a cell on each restricted point. */
export function drawRestriction(context: CanvasRenderingContext2D, geometry: Geometry, kind: RestrictionKind, points: readonly PointView[]) {
  const { cell, originX, originY } = geometry;
  context.save();
  context.fillStyle = FILL[kind];
  for (const point of points) context.fillRect(Math.round(originX + (point.x - 0.5) * cell), Math.round(originY + (point.y - 0.5) * cell), cell, cell);
  context.restore();
}

/** The rectangle being dragged, see-through, in its kind's colour. */
export function drawDragRect(context: CanvasRenderingContext2D, kind: RestrictionKind, from: Pixel, to: Pixel, dpr: number) {
  const x = Math.min(from[0], to[0]);
  const y = Math.min(from[1], to[1]);
  const width = Math.abs(to[0] - from[0]);
  const height = Math.abs(to[1] - from[1]);
  context.save();
  context.fillStyle = FILL[kind];
  context.fillRect(x, y, width, height);
  context.strokeStyle = EDGE[kind];
  context.lineWidth = Math.max(1, dpr);
  context.strokeRect(x, y, width, height);
  context.restore();
}

/**
 * 终局点目 as Java draws it: a square of 0.6 cell in the owner's colour on every point, a red dot
 * where nobody scores. A dead stone shows its opponent's square.
 */
export function drawScoring(context: CanvasRenderingContext2D, geometry: Geometry, ownership: readonly (ColorView | null)[]) {
  const { cell, originX, originY, columns } = geometry;
  const side = Math.max(2, Math.round(cell * 0.6));
  context.save();
  ownership.forEach((owner, index) => {
    const cx = originX + (index % columns) * cell;
    const cy = originY + Math.floor(index / columns) * cell;
    if (owner === null) {
      context.fillStyle = "#e01e1e";
      context.beginPath();
      context.arc(cx, cy, Math.max(2, cell * 0.12), 0, Math.PI * 2);
      context.fill();
      return;
    }
    const left = Math.round(cx - side / 2);
    const top = Math.round(cy - side / 2);
    context.fillStyle = owner === "Black" ? "rgba(0, 0, 0, 0.85)" : "rgba(255, 255, 255, 0.92)";
    context.fillRect(left, top, side, side);
    if (owner === "White") {
      context.strokeStyle = "rgba(0, 0, 0, 0.55)";
      context.lineWidth = 1;
      context.strokeRect(left + 0.5, top + 0.5, side - 1, side - 1);
    }
  });
  context.restore();
}
