import type { AnalysisView, BoardView, CandidateView, ColorView, PointView } from "@/api/backend";
import { candidateTier, columnLabel, formatScore, formatVisits, formatWinrate, forMover, rowLabel, winrateLoss } from "./format";
import { type Geometry, pointIndex, pointToPixel, starPoints } from "./geometry";
import type { Palette } from "./palette";

const FONT = "system-ui, -apple-system, 'Segoe UI', sans-serif";
const STONE_RADIUS = 0.48;
const CANDIDATE_RADIUS = 0.46;

/** Sizes the backing store for the device pixel ratio and maps drawing to CSS pixels. */
export function prepare(canvas: HTMLCanvasElement, geometry: Geometry, width: number, height: number) {
  // Nothing to draw before the first layout (and never in jsdom, which has no canvas).
  if (geometry.cell <= 0) return null;
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) return null;
  context.setTransform(geometry.dpr, 0, 0, geometry.dpr, 0, 0);
  context.clearRect(0, 0, geometry.cssWidth, geometry.cssHeight);
  return context;
}

export function drawBackground(context: CanvasRenderingContext2D, geometry: Geometry, palette: Palette, coordinates: boolean) {
  const { cell, originX, originY, margin, columns, rows } = geometry;
  if (cell <= 0) return;
  const left = originX - margin;
  const top = originY - margin;
  const width = cell * (columns - 1) + margin * 2;
  const height = cell * (rows - 1) + margin * 2;
  const wood = context.createLinearGradient(left, top, left + width, top + height);
  wood.addColorStop(0, palette.wood);
  wood.addColorStop(1, palette.woodEdge);
  context.fillStyle = wood;
  context.beginPath();
  context.roundRect(left, top, width, height, Math.min(8, cell * 0.2));
  context.fill();

  context.strokeStyle = palette.line;
  context.lineCap = "square";
  const thin = Math.min(1.6, Math.max(1, cell * 0.03));
  context.lineWidth = thin;
  context.beginPath();
  for (let x = 0; x < columns; x++) {
    const px = originX + x * cell;
    context.moveTo(px, originY);
    context.lineTo(px, originY + cell * (rows - 1));
  }
  for (let y = 0; y < rows; y++) {
    const py = originY + y * cell;
    context.moveTo(originX, py);
    context.lineTo(originX + cell * (columns - 1), py);
  }
  context.stroke();
  context.lineWidth = thin * 1.8;
  context.strokeRect(originX, originY, cell * (columns - 1), cell * (rows - 1));

  context.fillStyle = palette.line;
  for (const point of starPoints(columns, rows)) {
    const { x, y } = pointToPixel(geometry, point);
    context.beginPath();
    context.arc(x, y, Math.max(1.5, cell * 0.1), 0, Math.PI * 2);
    context.fill();
  }

  if (!coordinates) return;
  context.fillStyle = palette.coordinate;
  context.font = `500 ${Math.min(16, Math.max(8, cell * 0.34))}px ${FONT}`;
  context.textAlign = "center";
  context.textBaseline = "middle";
  const offset = margin * 0.55;
  for (let x = 0; x < columns; x++) {
    const label = columnLabel(x, columns);
    const px = originX + x * cell;
    context.fillText(label, px, originY - offset);
    context.fillText(label, px, originY + cell * (rows - 1) + offset);
  }
  for (let y = 0; y < rows; y++) {
    const label = rowLabel(y, rows);
    const py = originY + y * cell;
    context.fillText(label, originX - offset, py);
    context.fillText(label, originX + cell * (columns - 1) + offset, py);
  }
}

export function drawStones(context: CanvasRenderingContext2D, geometry: Geometry, palette: Palette, board: BoardView) {
  const { cell } = geometry;
  if (cell <= 0) return;
  board.stones.forEach((color, index) => {
    if (!color) return;
    const point = { x: index % board.width, y: Math.floor(index / board.width) };
    drawStone(context, geometry, palette, point, color, 1);
  });
  const last = board.last_move;
  if (last?.type === "Play") {
    const { x, y } = pointToPixel(geometry, last.data.point);
    context.strokeStyle = last.data.color === "Black" ? palette.lastMoveOnBlack : palette.lastMoveOnWhite;
    context.lineWidth = Math.max(1.5, cell * 0.065);
    context.beginPath();
    context.arc(x, y, cell * 0.22, 0, Math.PI * 2);
    context.stroke();
  }
}

function drawStone(
  context: CanvasRenderingContext2D,
  geometry: Geometry,
  palette: Palette,
  point: PointView,
  color: ColorView,
  alpha: number,
) {
  const { cell } = geometry;
  const { x, y } = pointToPixel(geometry, point);
  const radius = cell * STONE_RADIUS;
  const [light, dark] = color === "Black" ? palette.blackStone : palette.whiteStone;
  const gradient = context.createRadialGradient(x - radius * 0.32, y - radius * 0.36, radius * 0.08, x, y, radius * 1.05);
  gradient.addColorStop(0, light);
  gradient.addColorStop(1, dark);
  context.save();
  context.globalAlpha = alpha;
  if (alpha === 1) {
    context.shadowColor = palette.stoneShadow;
    context.shadowBlur = cell * 0.09;
    context.shadowOffsetX = cell * 0.03;
    context.shadowOffsetY = cell * 0.05;
  }
  context.fillStyle = gradient;
  context.beginPath();
  context.arc(x, y, radius, 0, Math.PI * 2);
  context.fill();
  context.restore();
}

export interface AnalysisLayer {
  board: BoardView;
  analysis: AnalysisView | null;
  ownership: boolean;
  maxCandidates: number;
  /** Candidate whose principal variation is previewed instead of the suggestions. */
  hover: PointView | null;
}

export function drawAnalysis(context: CanvasRenderingContext2D, geometry: Geometry, palette: Palette, layer: AnalysisLayer) {
  const { analysis, board } = layer;
  if (!analysis || geometry.cell <= 0) return;
  if (layer.ownership && analysis.ownership) drawOwnership(context, geometry, palette, board, analysis.ownership);
  const candidates = visibleCandidates(analysis, layer.maxCandidates);
  const hovered = layer.hover && candidates.find((candidate) => samePoint(candidate.point, layer.hover));
  if (hovered && hovered.pv.length > 0) {
    drawVariation(context, geometry, palette, hovered);
    return;
  }
  const best = candidates[0];
  if (!best) return;
  for (const candidate of candidates) {
    if (!candidate.point) continue;
    const tier = candidateTier(winrateLoss(candidate, best, board.to_play), candidate === best);
    drawCandidate(context, geometry, palette, candidate, tier, board.to_play);
  }
}

/** Top suggestions in engine order. Pass is listed by the engine but has no circle. */
export function visibleCandidates(analysis: AnalysisView, maxCandidates: number): CandidateView[] {
  return [...analysis.candidates].sort((a, b) => a.order - b.order).slice(0, Math.max(0, maxCandidates));
}

function drawOwnership(
  context: CanvasRenderingContext2D,
  geometry: Geometry,
  palette: Palette,
  board: BoardView,
  ownership: number[],
) {
  const { cell } = geometry;
  context.save();
  ownership.forEach((value, index) => {
    const strength = Math.min(1, Math.abs(value));
    if (strength < 0.08) return;
    const point = { x: index % board.width, y: Math.floor(index / board.width) };
    const stone = board.stones[pointIndex(board.width, point)];
    const owner: ColorView = value > 0 ? "Black" : "White";
    // On stones, only mark the ones the engine expects to be captured.
    if (stone && (stone === owner || strength < 0.4)) return;
    const side = stone ? cell * 0.32 : cell * (0.2 + 0.36 * strength);
    const { x, y } = pointToPixel(geometry, point);
    context.globalAlpha = stone ? 0.85 : 0.15 + 0.5 * strength;
    context.fillStyle = owner === "Black" ? palette.ownershipBlack : palette.ownershipWhite;
    context.fillRect(x - side / 2, y - side / 2, side, side);
  });
  context.restore();
}

function drawCandidate(
  context: CanvasRenderingContext2D,
  geometry: Geometry,
  palette: Palette,
  candidate: CandidateView,
  tier: number,
  toPlay: ColorView,
) {
  if (!candidate.point) return;
  const { cell } = geometry;
  const { x, y } = pointToPixel(geometry, candidate.point);
  const radius = cell * CANDIDATE_RADIUS;
  context.save();
  context.globalAlpha = 0.92;
  context.fillStyle = palette.tiers[Math.min(tier, palette.tiers.length - 1)] ?? palette.tiers[0] ?? "#888";
  context.beginPath();
  context.arc(x, y, radius, 0, Math.PI * 2);
  context.fill();
  context.globalAlpha = 1;
  if (tier === 0) {
    context.strokeStyle = palette.bestRing;
    context.lineWidth = Math.max(1.5, cell * 0.07);
    context.beginPath();
    context.arc(x, y, radius - context.lineWidth / 2, 0, Math.PI * 2);
    context.stroke();
  }
  if (cell >= 14) {
    context.fillStyle = palette.candidateText;
    context.textAlign = "center";
    context.textBaseline = "middle";
    const winrate = formatWinrate(forMover(candidate.winrate, toPlay, "winrate"));
    if (cell < 26) {
      context.font = `600 ${cell * 0.32}px ${FONT}`;
      context.fillText(winrate, x, y);
    } else {
      context.font = `650 ${cell * 0.27}px ${FONT}`;
      context.fillText(winrate, x, y - cell * 0.2);
      context.font = `500 ${cell * 0.2}px ${FONT}`;
      context.fillText(formatVisits(candidate.visits), x, y + cell * 0.04);
      context.fillText(formatScore(forMover(candidate.score_lead, toPlay, "score")), x, y + cell * 0.25);
    }
  }
  context.restore();
}

function drawVariation(context: CanvasRenderingContext2D, geometry: Geometry, palette: Palette, candidate: CandidateView) {
  const { cell } = geometry;
  candidate.pv.forEach((move, index) => {
    if (move.type !== "Play") return;
    drawStone(context, geometry, palette, move.data.point, move.data.color, 0.78);
    const { x, y } = pointToPixel(geometry, move.data.point);
    context.fillStyle = move.data.color === "Black" ? palette.lastMoveOnBlack : palette.lastMoveOnWhite;
    context.font = `600 ${cell * (index < 9 ? 0.42 : 0.34)}px ${FONT}`;
    context.textAlign = "center";
    context.textBaseline = "middle";
    context.fillText(String(index + 1), x, y + cell * 0.02);
  });
}

function samePoint(a: PointView | null, b: PointView | null): boolean {
  return a !== null && b !== null && a.x === b.x && a.y === b.y;
}
