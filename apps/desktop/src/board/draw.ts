import type { AnalysisView, BoardView, CandidateView, ColorView, PointView } from "@/api/backend";
import { candidateTier, columnLabel, formatScore, formatVisits, formatWinrate, forMover, rowLabel, winrateLoss } from "./format";
import { type Geometry, pointIndex, pointToPixel, starPoints } from "./geometry";
import type { Palette } from "./palette";

const FONT = "-apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI', 'PingFang SC', 'Microsoft YaHei', sans-serif";
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
  const radius = Math.min(6, cell * 0.16);
  const wood = context.createLinearGradient(left, top, left + width, top + height);
  wood.addColorStop(0, palette.wood);
  wood.addColorStop(1, palette.woodEdge);
  context.save();
  context.shadowColor = palette.boardShadow;
  context.shadowBlur = Math.max(12, cell * 0.9);
  context.shadowOffsetY = Math.max(4, cell * 0.3);
  context.fillStyle = wood;
  context.beginPath();
  context.roundRect(left, top, width, height, radius);
  context.fill();
  context.restore();
  drawGrain(context, left, top, width, height, radius, palette);

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

/**
 * Straight-grained kaya: long, faint, slightly wavy fibres plus a soft edge
 * vignette. Seeded, so every redraw paints the same board.
 */
function drawGrain(
  context: CanvasRenderingContext2D,
  left: number,
  top: number,
  width: number,
  height: number,
  radius: number,
  palette: Palette,
) {
  context.save();
  context.beginPath();
  context.roundRect(left, top, width, height, radius);
  context.clip();
  let seed = 20_261_030;
  const random = () => {
    seed = (seed * 16_807) % 2_147_483_647;
    return seed / 2_147_483_647;
  };
  context.strokeStyle = palette.woodGrain;
  const fibres = Math.max(18, Math.round(height / 4.5));
  for (let index = 0; index < fibres; index++) {
    const baseline = top + (index / fibres) * height + random() * 4;
    const amplitude = 0.8 + random() * 3.2;
    const wavelength = width * (0.5 + random() * 1.1);
    const phase = random() * Math.PI * 2;
    // Mostly hairlines; an occasional wider, still faint band reads as a growth ring.
    const ring = random() < 0.12;
    context.globalAlpha = ring ? 0.035 + random() * 0.03 : 0.012 + random() * 0.03;
    context.lineWidth = ring ? 2.5 + random() * 3 : 0.5 + random() * 0.9;
    context.beginPath();
    for (let x = 0; x <= width + 6; x += 6) {
      const y = baseline + Math.sin((x / wavelength) * Math.PI * 2 + phase) * amplitude;
      if (x === 0) context.moveTo(left + x, y);
      else context.lineTo(left + x, y);
    }
    context.stroke();
  }
  context.globalAlpha = 1;
  const vignette = context.createRadialGradient(
    left + width / 2,
    top + height / 2,
    Math.min(width, height) * 0.35,
    left + width / 2,
    top + height / 2,
    Math.max(width, height) * 0.75,
  );
  vignette.addColorStop(0, "rgba(0, 0, 0, 0)");
  vignette.addColorStop(1, palette.woodVignette);
  context.fillStyle = vignette;
  context.fillRect(left, top, width, height);
  context.restore();
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
    context.lineWidth = Math.max(1.6, cell * 0.075);
    context.beginPath();
    context.arc(x, y, cell * 0.19, 0, Math.PI * 2);
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
  const [light, middle, dark] = color === "Black" ? palette.blackStone : palette.whiteStone;
  const gradient = context.createRadialGradient(x - radius * 0.36, y - radius * 0.42, radius * 0.05, x, y, radius * 1.02);
  gradient.addColorStop(0, light);
  gradient.addColorStop(0.42, middle);
  gradient.addColorStop(1, dark);
  context.save();
  context.globalAlpha = alpha;
  if (alpha === 1) {
    context.shadowColor = palette.stoneShadow;
    context.shadowBlur = cell * 0.13;
    context.shadowOffsetX = cell * 0.035;
    context.shadowOffsetY = cell * 0.065;
  }
  context.fillStyle = gradient;
  context.beginPath();
  context.arc(x, y, radius, 0, Math.PI * 2);
  context.fill();
  context.shadowColor = "transparent";
  if (color === "White") {
    // A hairline keeps white stones crisp against the light wood.
    context.strokeStyle = palette.whiteStoneEdge;
    context.lineWidth = Math.max(0.6, cell * 0.018);
    context.stroke();
  }
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
  context.shadowColor = palette.candidateShadow;
  context.shadowBlur = cell * 0.12;
  context.shadowOffsetY = cell * 0.03;
  context.globalAlpha = 0.94;
  context.fillStyle = palette.tiers[Math.min(tier, palette.tiers.length - 1)] ?? palette.tiers[0] ?? "#888";
  context.beginPath();
  context.arc(x, y, radius, 0, Math.PI * 2);
  context.fill();
  context.shadowColor = "transparent";
  context.globalAlpha = 1;
  if (tier === 0) {
    context.strokeStyle = palette.bestRing;
    context.lineWidth = Math.max(1.5, cell * 0.06);
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
      context.font = `700 ${cell * 0.31}px ${FONT}`;
      context.fillText(winrate, x, y + cell * 0.01);
    } else {
      context.font = `700 ${cell * 0.27}px ${FONT}`;
      context.fillText(winrate, x, y - cell * 0.19);
      context.globalAlpha = 0.78;
      context.font = `600 ${cell * 0.185}px ${FONT}`;
      context.fillText(formatVisits(candidate.visits), x, y + cell * 0.045);
      context.fillText(formatScore(forMover(candidate.score_lead, toPlay, "score")), x, y + cell * 0.25);
      context.globalAlpha = 1;
    }
  }
  if (cell >= 22) {
    // The engine's ranking, top right of the circle, as LizzieYzy Next shows it.
    context.fillStyle = palette.candidateOrder;
    context.font = `700 ${cell * 0.2}px ${FONT}`;
    context.textAlign = "center";
    context.textBaseline = "middle";
    context.fillText(String(candidate.order + 1), x + radius * 0.9, y - radius * 0.9);
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
