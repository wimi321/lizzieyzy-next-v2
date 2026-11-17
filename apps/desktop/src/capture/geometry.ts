import type { CaptureCell, CaptureCells, CaptureGrid, CapturePoint } from "@/api/backend";

/** A line in screenshot pixels. */
export interface Line {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

export interface Size {
  width: number;
  height: number;
}

/** The board sizes the captured problem can be placed on, as the confirm bar offers them. */
export const BOARD_SIZES = [9, 13, 19] as const;

/** A PNG's pixel size from its IHDR header, or null when the bytes are not a PNG. */
export function pngSize(bytes: ArrayBuffer): Size | null {
  const view = new DataView(bytes);
  const signature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  if (view.byteLength < 24 || signature.some((byte, index) => view.getUint8(index) !== byte)) return null;
  const width = view.getUint32(16);
  const height = view.getUint32(20);
  return width > 0 && height > 0 ? { width, height } : null;
}

/** A pointer position over the stretched screenshot, in its own pixels. */
export function toImage(clientX: number, clientY: number, box: { left: number; top: number; width: number; height: number }, image: Size): CapturePoint {
  const scaleX = box.width > 0 ? image.width / box.width : 1;
  const scaleY = box.height > 0 ? image.height / box.height : 1;
  return { x: (clientX - box.left) * scaleX, y: (clientY - box.top) * scaleY };
}

/**
 * Java's preview after the first click: five lines each way from the corner, spaced as far as the
 * pointer is from it, towards the pointer.
 */
export function previewLines(first: CapturePoint, pointer: CapturePoint): Line[] {
  const dx = pointer.x - first.x;
  const dy = pointer.y - first.y;
  const lines: Line[] = [];
  for (let k = 0; k < 5; k++) {
    lines.push({ x1: first.x + k * dx, y1: first.y, x2: first.x + k * dx, y2: first.y + 4 * dy });
    lines.push({ x1: first.x, y1: first.y + k * dy, x2: first.x + 4 * dx, y2: first.y + k * dy });
  }
  return lines;
}

/** The intersection at a row and column of the fitted grid (the origin is its top-left one). */
export function intersection(grid: CaptureGrid, row: number, column: number): CapturePoint {
  return { x: grid.origin.x + column * grid.gap_x, y: grid.origin.y + row * grid.gap_y };
}

/** Every line of the fitted grid. */
export function gridLines(grid: CaptureGrid): Line[] {
  const last = intersection(grid, grid.rows - 1, grid.columns - 1);
  const lines: Line[] = [];
  for (let column = 0; column < grid.columns; column++) {
    const x = grid.origin.x + column * grid.gap_x;
    lines.push({ x1: x, y1: grid.origin.y, x2: x, y2: last.y });
  }
  for (let row = 0; row < grid.rows; row++) {
    const y = grid.origin.y + row * grid.gap_y;
    lines.push({ x1: grid.origin.x, y1: y, x2: last.x, y2: y });
  }
  return lines;
}

/** The intersection a click lands on, within half a cell; null outside the grid. */
export function cellAt(grid: CaptureGrid, point: CapturePoint): { row: number; column: number } | null {
  const column = Math.round((point.x - grid.origin.x) / grid.gap_x);
  const row = Math.round((point.y - grid.origin.y) / grid.gap_y);
  if (row < 0 || column < 0 || row >= grid.rows || column >= grid.columns) return null;
  const at = intersection(grid, row, column);
  return Math.abs(point.x - at.x) <= grid.gap_x / 2 && Math.abs(point.y - at.y) <= grid.gap_y / 2 ? { row, column } : null;
}

const NEXT: Record<CaptureCell, CaptureCell> = { Empty: "Black", Black: "White", White: "Empty" };

/** A correction: empty → black → white → empty at one intersection. */
export function cycled(cells: CaptureCells, row: number, column: number): CaptureCells {
  const index = row * cells.columns + column;
  const current = cells.cells[index];
  if (current === undefined) return cells;
  const next = [...cells.cells];
  next[index] = NEXT[current];
  return { ...cells, cells: next };
}

/** Whether the problem fits on a board of this size, placed at its corner. */
export function fits(cells: CaptureCells, size: number): boolean {
  return cells.columns <= size && cells.rows <= size;
}

/**
 * The current game's size when it is offered and the problem fits there; otherwise the smallest
 * offered size that fits; null when none does.
 */
export function defaultBoardSize(cells: CaptureCells, preferred: number | null): number | null {
  if (preferred !== null && (BOARD_SIZES as readonly number[]).includes(preferred) && fits(cells, preferred)) return preferred;
  return BOARD_SIZES.find((size) => fits(cells, size)) ?? null;
}

/** The main window leaves the current game's size here for the capture window, which cannot see the game. */
export const BOARD_SIZE_KEY = "lizzieyzy-capture-board-size";

export function storedBoardSize(storage: Pick<Storage, "getItem"> | null): number | null {
  try {
    const value = Number(storage?.getItem(BOARD_SIZE_KEY));
    return Number.isInteger(value) && value > 0 ? value : null;
  } catch {
    return null;
  }
}

/** Two clicks make cells only when they differ both ways (Java fails otherwise). */
export function makesCells(first: CapturePoint, second: CapturePoint): boolean {
  return first.x !== second.x && first.y !== second.y;
}
