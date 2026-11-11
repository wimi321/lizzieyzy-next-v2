import type { AnalysisView, BoardView, CandidateView, ColorView, MoveRank, PointView, VariationPreviewView } from "@/api/backend";
import { columnLabel, forMover, rowLabel } from "./format";
import { type Geometry, pointIndex, pointToPixel, starPoints } from "./geometry";
import {
  blunderLevel,
  css,
  fitFontSize,
  lineSpan,
  lineWidths,
  playoutsText,
  RANK_COLORS,
  scoreText,
  shownSuggestions,
  starRadius,
  suggestionAlphaRatio,
  suggestionColor,
  winrateText,
} from "./suggestion";

/**
 * LizzieYzy Next's `BoardRenderer`, japanese style and default theme, drawn in device pixels:
 * Java lays the board out in device pixels on high-DPI screens, so the canvas does too.
 */

/** Java's logical `Dialog` font: Lucida Grande on macOS, Arial on Windows, DejaVu on Linux. */
export const UI_FONT = '"Lucida Grande", Arial, "DejaVu Sans", "PingFang SC", "Microsoft YaHei", sans-serif';
/** Open Sans Semibold, which Java ships for winrates and scores. */
export const NUMBER_FONT = `"Lizzie Open Sans", ${UI_FONT}`;

export interface BoardImages {
  board: CanvasImageSource;
  black: CanvasImageSource;
  white: CanvasImageSource;
}

type Context = CanvasRenderingContext2D;
const TAU = Math.PI * 2;

/** Sizes the backing store to the geometry and clears it. */
export function prepare(canvas: HTMLCanvasElement, geometry: Geometry): Context | null {
  // Nothing to draw before the first layout (and never in jsdom, which has no canvas).
  if (geometry.cell <= 0) return null;
  if (canvas.width !== geometry.cssWidth) canvas.width = geometry.cssWidth;
  if (canvas.height !== geometry.cssHeight) canvas.height = geometry.cssHeight;
  const context = canvas.getContext("2d");
  if (!context) return null;
  context.setTransform(1, 0, 0, 1, 0, 0);
  context.clearRect(0, 0, canvas.width, canvas.height);
  return context;
}

/** Java's `fillOval(x - r, y - r, 2r + 1, 2r + 1)`: a disc of radius r + ½ about the pixel centre. */
function disc(context: Context, x: number, y: number, radius: number) {
  context.beginPath();
  context.arc(x, y, radius + 0.5, 0, TAU);
}

function wood(context: Context, geometry: Geometry, images: BoardImages): CanvasPattern | string {
  const pattern = context.createPattern(images.board, "repeat");
  if (!pattern) return "#dbb973";
  pattern.setTransform(new DOMMatrix().translate(geometry.boardX, geometry.boardY));
  return pattern;
}

// ---------------------------------------------------------------- text

const widths = new Map<string, number>();

function widthAt100(context: Context, family: string, text: string): number {
  const key = `${family}\n${text}`;
  let width = widths.get(key);
  if (width === undefined) {
    context.font = `100px ${family}`;
    width = context.measureText(text).width;
    // A font that is still loading measures with its fallback; do not remember that.
    if (widths.size > 4000) widths.clear();
    if (fontsSettled) widths.set(key, width);
  }
  return width;
}

let fontsSettled = false;
/** Called once the web fonts are in, so cached measurements are the real ones. */
export function settleFonts() {
  widths.clear();
  fontsSettled = true;
}

interface TextBox {
  left: number;
  baseline: number;
  width: number;
  /** Java's `ascent - descent`, the height it centres text by. */
  height: number;
}

/**
 * `BoardRenderer.drawString`: the largest font within the limits, centred on x.
 * `anchor` says where y is: the text's middle, its baseline ("above", Java's aboveOrBelow 1)
 * or its top ("below").
 */
function text(
  context: Context,
  string: string,
  x: number,
  y: number,
  family: string,
  maximumHeight: number,
  maximumWidth: number,
  anchor: "middle" | "above" | "below" = "middle",
  paint = true,
): TextBox {
  const size = fitFontSize(widthAt100(context, family, string), maximumHeight, maximumWidth);
  context.font = `${size}px ${family}`;
  context.textAlign = "left";
  context.textBaseline = "alphabetic";
  const metrics = context.measureText(string);
  const height = roundUp(metrics.fontBoundingBoxAscent) - roundUp(metrics.fontBoundingBoxDescent);
  const half = Math.trunc(height / 2);
  const width = Math.round(metrics.width);
  // Java places text on whole pixels; x and y arrive as pixel centres or as whole pixels.
  const baseline = Math.floor(y) + half + (anchor === "below" ? half : anchor === "above" ? -half : 0);
  const left = Math.floor(x) - Math.trunc(width / 2);
  if (paint) context.fillText(string, left, baseline);
  return { left, baseline, width, height };
}

/** `FontMetrics` rounds an ascent or descent up unless it is within 0.05 of the pixel below. */
const roundUp = (value: number) => Math.floor(value + 0.95);

// ---------------------------------------------------------------- board

export interface Names {
  black: string;
  white: string;
}

export interface GobanLayer {
  coordinates: boolean;
  /** Player names under the bottom line; they replace the bottom coordinates. */
  names: Names | null;
  toPlay: ColorView;
  dpr: number;
}

export function drawGoban(context: Context, geometry: Geometry, images: BoardImages, layer: GobanLayer) {
  const { boardX: left, boardY: top, boardWidth: width, boardHeight: height, cell, originX, originY, columns, rows } = geometry;
  if (cell <= 0) return;
  context.fillStyle = wood(context, geometry, images);
  context.fillRect(left, top, width, height);

  // GlassEffectRenderer.drawBoardLightGradient: warm light from the top left.
  const light = context.createLinearGradient(left, top, left + width, top + height);
  light.addColorStop(0, "rgba(255, 255, 230, 0.098)");
  light.addColorStop(1, "rgba(0, 0, 20, 0.098)");
  context.fillStyle = light;
  context.fillRect(left, top, width, height);

  // drawBoardInnerShadow: the board sits slightly below its frame.
  const depth = Math.max(Math.trunc(width / 80), 4);
  const edge = (x0: number, y0: number, x1: number, y1: number, alpha: number, x: number, y: number, w: number, h: number) => {
    const shade = context.createLinearGradient(x0, y0, x1, y1);
    shade.addColorStop(0, `rgba(0, 0, 0, ${alpha / 255})`);
    shade.addColorStop(1, "rgba(0, 0, 0, 0)");
    context.fillStyle = shade;
    context.fillRect(x, y, w, h);
  };
  edge(0, top, 0, top + depth, 30, left, top, width, depth);
  edge(left, 0, left + depth, 0, 20, left, top, depth, height);
  edge(0, top + height, 0, top + height - depth, 20, left, top + height - depth, width, depth);
  edge(left + width, 0, left + width - depth, 0, 15, left + width - depth, top, depth, height);

  // Whole pixels, no antialiasing: a soft grid is the first thing that looks wrong on a board.
  const stroke = lineWidths(width, geometry.available);
  const firstX = Math.floor(originX);
  const firstY = Math.floor(originY);
  const spanX = cell * (columns - 1);
  const spanY = cell * (rows - 1);
  const [borderOffset, borderSize] = lineSpan(stroke.border);
  context.fillStyle = "#000";
  for (let x = 0; x < columns; x++) {
    const [offset, size] = lineSpan(x === 0 || x === columns - 1 ? stroke.border : stroke.inner);
    context.fillRect(firstX + x * cell + offset, firstY + borderOffset, size, spanY + borderSize);
  }
  for (let y = 0; y < rows; y++) {
    const [offset, size] = lineSpan(y === 0 || y === rows - 1 ? stroke.border : stroke.inner);
    context.fillRect(firstX + borderOffset, firstY + y * cell + offset, spanX + borderSize, size);
  }

  context.fillStyle = "#000";
  const star = starRadius(width, height);
  for (const point of starPoints(columns, rows)) {
    const { x, y } = pointToPixel(geometry, point);
    disc(context, x, y, star);
    context.fill();
  }

  if (layer.coordinates) drawCoordinates(context, geometry, layer.names === null);
  if (layer.names) drawNames(context, geometry, images, layer.names, layer.toPlay, layer.dpr);
}

function drawCoordinates(context: Context, geometry: Geometry, bottom: boolean) {
  const { boardX, boardY, boardWidth, boardHeight, margin, stoneRadius, columns, rows } = geometry;
  const inset = Math.trunc((margin * 4) / 10);
  const maximumHeight = Math.trunc((stoneRadius * 4) / 5);
  context.fillStyle = "#000";
  for (let x = 0; x < columns; x++) {
    const label = columnLabel(x, columns);
    const px = pointToPixel(geometry, { x, y: 0 }).x;
    text(context, label, px, boardY + inset, UI_FONT, maximumHeight, stoneRadius);
    if (bottom) text(context, label, px, boardY + boardHeight - inset, UI_FONT, maximumHeight, stoneRadius);
  }
  for (let y = 0; y < rows; y++) {
    const label = rowLabel(y, rows);
    const py = pointToPixel(geometry, { x: 0, y }).y;
    text(context, label, boardX + inset, py, UI_FONT, maximumHeight, stoneRadius);
    text(context, label, boardX + boardWidth - inset, py, UI_FONT, maximumHeight, stoneRadius);
  }
}

function truncate(context: Context, name: string, maximumWidth: number): string {
  if (context.measureText(name).width <= maximumWidth) return name;
  const letters = Array.from(name);
  let end = letters.length;
  while (end > 1 && context.measureText(`${letters.slice(0, end).join("")}…`).width > maximumWidth) end--;
  return `${letters.slice(0, end).join("").trimEnd()}…`;
}

/**
 * `BoardNamePlate` without its plate: two small stones in the middle, a name on either side.
 * Java's 28 px cap is in device pixels on the maintainer's Retina screen, 14 px to the eye; the
 * same 14 px is kept on every screen.
 */
function drawNames(context: Context, geometry: Geometry, images: BoardImages, names: Names, toPlay: ColorView, dpr: number) {
  const { boardX, boardY, boardWidth, boardHeight, margin, stoneRadius: r, cell, columns } = geometry;
  const size = Math.min(Math.round(14 * dpr), Math.trunc((margin * 53) / 100));
  if (size < 4) return;
  const center = boardX + Math.trunc(boardWidth / 2);
  const top = boardY + boardHeight - margin;
  context.font = `${size}px ${UI_FONT}`;
  context.textBaseline = "alphabetic";
  const metrics = context.measureText("Ag");
  const fontHeight = Math.round(metrics.fontBoundingBoxAscent + metrics.fontBoundingBoxDescent);
  const baseline = top + Math.trunc((r * 13) / 8) + Math.trunc((fontHeight + 2) / 4);
  const maximumWidth = Math.trunc(cell * columns * 0.43);
  const gap = Math.trunc((margin * 4) / 5);

  const black = truncate(context, names.black, maximumWidth);
  context.textAlign = "right";
  context.fillStyle = "#000";
  context.fillText(black, center - gap, baseline);

  const white = truncate(context, names.white, maximumWidth);
  context.textAlign = "left";
  context.lineJoin = "round";
  context.lineWidth = Math.max(1.5, dpr);
  context.strokeStyle = "#000";
  context.strokeText(white, center + gap + 1, baseline);
  context.fillStyle = "#fff";
  context.fillText(white, center + gap + 1, baseline);
  context.textAlign = "left";

  const stone = Math.trunc((r * 5) / 4);
  const blackStone = () => context.drawImage(images.black, center - Math.trunc((r * 9) / 10), top + r, stone, stone);
  const whiteStone = () => context.drawImage(images.white, center - Math.trunc((r * 3) / 10), top + r, stone, stone);
  context.imageSmoothingQuality = "high";
  // The side to move lies on top.
  if (toPlay === "Black") {
    whiteStone();
    blackStone();
  } else {
    blackStone();
    whiteStone();
  }
}

// ---------------------------------------------------------------- stones

const sprites = new Map<string, HTMLCanvasElement>();

function sprite(key: string, size: number, paint: (context: Context) => void): HTMLCanvasElement {
  let canvas = sprites.get(key);
  if (!canvas) {
    if (sprites.size > 24) sprites.clear();
    canvas = document.createElement("canvas");
    canvas.width = size;
    canvas.height = size;
    const context = canvas.getContext("2d");
    if (context) paint(context);
    sprites.set(key, canvas);
  }
  return canvas;
}

/** The stone image scaled once per size, with `GlassEffectRenderer.drawStoneHighlight` on top. */
function stoneSprite(images: BoardImages, color: ColorView, r: number): HTMLCanvasElement {
  const size = 2 * r + 1;
  return sprite(`stone:${color}:${r}`, size, (context) => {
    const black = color === "Black";
    context.imageSmoothingQuality = "high";
    context.drawImage(black ? images.black : images.white, 0, 0, size, size);
    const c = r;
    const highlight = context.createRadialGradient(c - r * 0.3, c - r * 0.3, 0, c - r * 0.3, c - r * 0.3, r * 0.6);
    highlight.addColorStop(0, `rgba(255, 255, 255, ${(black ? 70 : 180) / 255})`);
    highlight.addColorStop(0.6, `rgba(255, 255, 255, ${(black ? 20 : 60) / 255})`);
    highlight.addColorStop(1, "rgba(255, 255, 255, 0)");
    const rim = context.createRadialGradient(c, c, 0, c, c, r);
    rim.addColorStop(0.85, "rgba(255, 255, 255, 0)");
    rim.addColorStop(0.95, `rgba(255, 255, 255, ${(black ? 25 : 50) / 255})`);
    rim.addColorStop(1, "rgba(255, 255, 255, 0)");
    for (const paint of [highlight, rim]) {
      context.fillStyle = paint;
      context.beginPath();
      context.arc(c, c, r, 0, TAU);
      context.fill();
    }
  });
}

/** `drawShadowCache` at the default shadow size of 85: contact shade, soft halo, a cast to the lower right. */
function shadowSprite(r: number): { canvas: HTMLCanvasElement; half: number } {
  const scaled = r * 0.85;
  const near = Math.trunc(scaled * 0.2) || 1;
  const far = Math.trunc(scaled * 0.17) || 1;
  const ambient = Math.trunc(scaled * 0.25) || 1;
  const half = r + near + ambient * 2 + 2;
  const canvas = sprite(`shadow:${r}`, half * 2 + 1, (context) => {
    const c = half + 0.5;
    const fill = (x: number, y: number, radius: number, stops: Array<[number, string]>) => {
      const gradient = context.createRadialGradient(x, y, 0, x, y, radius);
      for (const [offset, color] of stops) gradient.addColorStop(offset, color);
      context.fillStyle = gradient;
      context.beginPath();
      context.arc(x, y, radius, 0, TAU);
      context.fill();
    };
    fill(c + ambient / 2, c + ambient / 2, r + near + ambient, [
      [0.5, "rgba(0, 0, 0, 0)"],
      [0.8, "rgba(0, 0, 0, 0.118)"],
      [1, "rgba(0, 0, 0, 0)"],
    ]);
    fill(c, c, r + near, [
      [0.3, "rgba(50, 50, 50, 0.627)"],
      [1, "rgba(50, 50, 50, 0)"],
    ]);
    fill(c + near, c + near, r + far, [
      [0.5, "rgba(0, 0, 0, 0.588)"],
      [1, "rgba(0, 0, 0, 0)"],
    ]);
  });
  return { canvas, half };
}

export function drawStones(context: Context, geometry: Geometry, images: BoardImages, board: BoardView) {
  const r = geometry.stoneRadius;
  if (geometry.cell <= 0) return;
  const placed: Array<{ x: number; y: number; color: ColorView }> = [];
  board.stones.forEach((color, index) => {
    if (!color) return;
    const { x, y } = pointToPixel(geometry, { x: index % board.width, y: Math.floor(index / board.width) });
    placed.push({ x, y, color });
  });
  const shadow = shadowSprite(r);
  for (const { x, y } of placed) context.drawImage(shadow.canvas, x - 0.5 - shadow.half, y - 0.5 - shadow.half);
  for (const { x, y, color } of placed) context.drawImage(stoneSprite(images, color, r), x - 0.5 - r, y - 0.5 - r);
}

// ---------------------------------------------------------------- analysis

/** The game's next move and, once analysed, the position after it (Black's perspective). */
export interface NextMove {
  point: PointView;
  after: { winrate: number; score_lead: number } | null;
}

export interface OverlayLayer {
  board: BoardView;
  analysis: AnalysisView | null;
  ownership: boolean;
  maxCandidates: number;
  hover: PointView | null;
  /** Rust's replay of the hovered candidate's line; null while not hovering or not yet answered. */
  variation: VariationPreviewView | null;
  /** How good the last move was; null when it or the move before it has no analysis. */
  lastRank: MoveRank | null;
  /** The recorded continuations of this position, main line first. */
  next: NextMove[];
  passLabel: string;
  /** Shown on an empty game that has nothing else to show. */
  emptyHint: string | null;
  dpr: number;
}

const same = (a: PointView | null | undefined, b: PointView | null | undefined) => !!a && !!b && a.x === b.x && a.y === b.y;

export function drawOverlay(context: Context, geometry: Geometry, images: BoardImages, layer: OverlayLayer) {
  if (geometry.cell <= 0) return;
  const { board, analysis } = layer;
  if (layer.variation && layer.hover) {
    drawVariation(context, geometry, images, layer, layer.variation);
    return;
  }
  if (layer.ownership && analysis?.ownership) drawEstimate(context, geometry, board, analysis.ownership);
  drawLastMove(context, geometry, layer);
  const candidates = analysis ? shownSuggestions(analysis, layer.maxCandidates) : [];
  const main = layer.next[0];
  if (main && analysis && candidates.length > 0) drawNextMoveBlunder(context, geometry, images, layer, candidates, main);
  drawSuggestions(context, geometry, images, layer, candidates);
  drawNextMoves(context, geometry, layer);
  if (layer.emptyHint && candidates.length === 0) drawEmptyHint(context, geometry, layer.emptyHint, layer.dpr);
}

/** `drawKataEstimateByTransparent`: small squares, darker or lighter with the owner's certainty. */
function drawEstimate(context: Context, geometry: Geometry, board: BoardView, ownership: number[]) {
  const { cell } = geometry;
  const blackHalf = Math.round(cell / 8);
  const whiteHalf = Math.round(cell / 6);
  ownership.forEach((value, index) => {
    const alpha = Math.min(255, Math.trunc(Math.abs(value) * 255));
    if (alpha <= 0) return;
    const center = pointToPixel(geometry, { x: index % board.width, y: Math.floor(index / board.width) });
    const x = center.x - 0.5;
    const y = center.y - 0.5;
    if (value > 0) {
      const onBlack = board.stones[index] === "Black";
      const gray = 127 - Math.trunc((alpha - 1) / 2);
      context.fillStyle = onBlack ? `rgb(${gray}, ${gray}, ${gray})` : `rgba(0, 0, 0, ${alpha / 255})`;
      context.fillRect(x - blackHalf, y - blackHalf, blackHalf * 2, blackHalf * 2);
    } else {
      context.fillStyle = `rgba(255, 255, 255, ${alpha / 255})`;
      context.fillRect(x - whiteHalf, y - whiteHalf, whiteHalf * 2, whiteHalf * 2);
    }
  });
}

/** The move-rank mark: a disc in the rank's colour inside a ring of the opposite stone colour. */
function drawLastMove(context: Context, geometry: Geometry, layer: OverlayLayer) {
  const last = layer.board.last_move;
  if (!last) return;
  const { cell, stoneRadius: r } = geometry;
  if (last.type === "Pass") {
    const x = geometry.boardX + Math.trunc(geometry.boardWidth / 2) + 0.5;
    const y = geometry.boardY + Math.trunc(geometry.boardHeight / 2) + 0.5;
    const blackToPlay = layer.board.to_play === "Black";
    context.fillStyle = blackToPlay ? "rgba(255, 255, 255, 0.314)" : "rgba(0, 0, 0, 0.314)";
    disc(context, x, y, Math.trunc((r * 5) / 2));
    context.fill();
    context.fillStyle = blackToPlay ? "rgba(0, 0, 0, 0.784)" : "rgba(255, 255, 255, 0.784)";
    text(context, layer.passLabel, x, y, UI_FONT, r * 3, (r * 9) / 2);
    return;
  }
  const { x, y } = pointToPixel(geometry, last.data.point);
  const radius = Math.round(cell * 0.22);
  if (layer.lastRank) {
    context.fillStyle = RANK_COLORS[layer.lastRank];
    disc(context, x, y, radius);
    context.fill();
  }
  context.strokeStyle = last.data.color === "Black" ? "#fff" : "#000";
  context.lineWidth = Math.max(1, radius / 5);
  disc(context, x, y, radius);
  context.stroke();
}

function suggestionRows(context: Context, geometry: Geometry, x: number, y: number, candidate: CandidateView, toPlay: ColorView) {
  const { cell, stoneRadius: r } = geometry;
  const winrate = forMover(candidate.winrate, toPlay, "winrate");
  const narrow = winrate * 100 < 10;
  text(context, winrateText(winrate), x, y - Math.round(cell * (narrow ? 0.127 : 0.125)), NUMBER_FONT, cell * (narrow ? 0.36 : 0.35), cell * 0.67, "above");
  text(context, playoutsText(candidate.visits), x, y + Math.round(cell * 0.18), UI_FONT, cell * 0.34, candidate.visits < 1000 ? r * 1.3 : r * 1.8, "above");
  const score = scoreText(forMover(candidate.score_lead, toPlay, "score"));
  text(context, score, x, y + Math.round(cell * 0.435), NUMBER_FONT, (geometry.available * 0.273) / (geometry.columns - 1), r * 1.6, "above");
}

/** `drawLeelazSuggestions`, weakest first so that stronger moves' rings and badges stay whole. */
function drawSuggestions(context: Context, geometry: Geometry, images: BoardImages, layer: OverlayLayer, candidates: CandidateView[]) {
  const best = candidates[0];
  if (!best) return;
  const { cell, stoneRadius: r } = geometry;
  const maxVisits = Math.max(...candidates.map((candidate) => candidate.visits));
  const texture = wood(context, geometry, images);
  for (let index = candidates.length - 1; index >= 0; index--) {
    const candidate = candidates[index]!;
    if (!candidate.point) continue;
    const { x, y } = pointToPixel(geometry, candidate.point);
    const isBest = candidate === best;
    const isNext = layer.next.some((next) => same(next.point, candidate.point));
    const ratio = maxVisits > 0 ? candidate.visits / maxVisits : 0;
    const faint = ratio < 0.05 && !isNext && !same(layer.hover, candidate.point);

    // Wood first, so the grid does not show through the translucent fill.
    context.globalAlpha = faint ? 0.8 : 1;
    context.fillStyle = texture;
    disc(context, x, y, r + 1);
    context.fill();
    context.globalAlpha = 1;
    context.fillStyle = css(suggestionColor(candidate.visits, maxVisits, isBest));
    disc(context, x, y, r + 1);
    context.fill();

    if (isBest) {
      context.strokeStyle = `rgba(0, 0, 255, ${isNext ? 170 / 255 : 1})`;
      context.lineWidth = (r + 2) / 15;
      disc(context, x, y, r + 2);
      context.stroke();
    } else if (!isNext) {
      context.strokeStyle = `rgba(0, 0, 0, ${(48 + 48 * suggestionAlphaRatio(ratio)) / 255})`;
      context.lineWidth = (r + 1) / 26.5;
      disc(context, x, y, r + 1);
      context.stroke();
    }

    context.fillStyle = "#000";
    suggestionRows(context, geometry, x, y, candidate, layer.board.to_play);

    // The engine's order in an orange tag at the top right, for the first nine moves.
    if (candidate.order <= 8) {
      const shift = isBest ? 1 : 0;
      const tagX = x + Math.round(cell * 0.43) + shift;
      const tagY = y - Math.round(cell * 0.358) - shift;
      const label = String(candidate.order + 1);
      const box = text(context, label, tagX, tagY, NUMBER_FONT, cell * 0.36, cell * 0.39, "above", false);
      context.fillStyle = "rgb(255, 200, 0)";
      context.fillRect(box.left, box.baseline - box.height, box.width, box.height + Math.max(1, Math.trunc(box.height / 12)));
      context.fillStyle = "#000";
      context.fillText(label, box.left, box.baseline);
    }
  }
}

/** A dashed ring in the mover's colour on every recorded continuation. */
function drawNextMoves(context: Context, geometry: Geometry, layer: OverlayLayer) {
  const r = geometry.stoneRadius;
  const width = Math.max(r / 5.5, 3);
  context.save();
  context.strokeStyle = layer.board.to_play === "Black" ? "#000" : "#fff";
  context.lineWidth = width;
  context.lineCap = "round";
  context.setLineDash([width * 2.2, width * 1.7]);
  for (const next of layer.next) {
    if (layer.board.stones[pointIndex(layer.board.width, next.point)]) continue;
    const { x, y } = pointToPixel(geometry, next.point);
    disc(context, x, y, r + 2);
    context.stroke();
  }
  context.restore();
}

/**
 * `drawNextMoveBlunder`: when the game's next move is not one the engine is seriously reading,
 * show what it leads to, coloured by how much it gives up against the best move.
 */
function drawNextMoveBlunder(
  context: Context,
  geometry: Geometry,
  images: BoardImages,
  layer: OverlayLayer,
  candidates: CandidateView[],
  next: NextMove,
) {
  if (!next.after || layer.board.stones[pointIndex(layer.board.width, next.point)]) return;
  const listed = candidates.find((candidate) => same(candidate.point, next.point));
  if (listed && listed.visits >= 30) return;
  const best = candidates[0]!;
  const toPlay = layer.board.to_play;
  const winrate = forMover(next.after.winrate, toPlay, "winrate");
  const score = forMover(next.after.score_lead, toPlay, "score");
  const level = blunderLevel(
    (forMover(best.winrate, toPlay, "winrate") - winrate) * 100,
    forMover(best.score_lead, toPlay, "score") - score,
  );
  const { cell, stoneRadius: r } = geometry;
  const { x, y } = pointToPixel(geometry, next.point);
  context.globalAlpha = 0.8;
  context.fillStyle = wood(context, geometry, images);
  disc(context, x, y, r + 1);
  context.fill();
  context.globalAlpha = 1;
  context.fillStyle = level === "bad" ? "rgba(220, 80, 30, 0.333)" : level === "doubtful" ? "rgba(220, 150, 30, 0.333)" : "rgba(0, 220, 0, 0.333)";
  disc(context, x, y, r + 1);
  context.fill();
  context.strokeStyle = toPlay === "Black" ? "#000" : "#fff";
  context.lineWidth = Math.max(r / 7, 2);
  disc(context, x, y, r + 2);
  context.stroke();
  context.fillStyle = level === "good" ? "#000" : "#fff";
  text(context, winrateText(winrate), x, y, NUMBER_FONT, cell * 0.39, cell * 0.815, "above");
  const rounded = Math.round(score * 10) / 10;
  const down = rounded <= -10 ? cell * 0.34 : (cell * 11) / 30;
  text(context, scoreText(score), x, y + Math.round(down), NUMBER_FONT, cell * 0.3, cell * 0.65, "above");
}

/** The sub-board's picture: a line's stones, numbered, over the position it leads to. */
export function drawLine(context: Context, geometry: Geometry, images: BoardImages, board: BoardView, variation: VariationPreviewView) {
  if (geometry.cell > 0) drawVariation(context, geometry, images, { board, analysis: null, hover: null }, variation);
}

/** `VariationPreviewGenerator`: the hovered line as numbered stones over Rust's replayed board. */
function drawVariation(
  context: Context,
  geometry: Geometry,
  images: BoardImages,
  layer: Pick<OverlayLayer, "board" | "analysis" | "hover">,
  variation: VariationPreviewView,
) {
  const { stoneRadius: r } = geometry;
  const { board } = layer;
  // Stones the line captures stay visible as ghosts.
  const texture = wood(context, geometry, images);
  board.stones.forEach((color, index) => {
    if (!color || variation.board.stones[index]) return;
    const { x, y } = pointToPixel(geometry, { x: index % board.width, y: Math.floor(index / board.width) });
    context.fillStyle = texture;
    disc(context, x, y, r);
    context.fill();
    context.fillStyle = color === "Black" ? "rgba(0, 0, 0, 0.314)" : "rgba(255, 255, 255, 0.431)";
    disc(context, x, y, r);
    context.fill();
  });

  if (layer.hover) {
    const { x, y } = pointToPixel(geometry, layer.hover);
    const glow = context.createRadialGradient(x, y, 0, x, y, r * 2);
    glow.addColorStop(0, "rgba(255, 255, 255, 0.118)");
    glow.addColorStop(0.5, "rgba(255, 255, 255, 0.039)");
    glow.addColorStop(1, "rgba(255, 255, 255, 0)");
    context.fillStyle = glow;
    context.beginPath();
    context.arc(x, y, r * 2, 0, TAU);
    context.fill();
  }

  const last = variation.stones.reduce((most, stone) => Math.max(most, stone.move_number), 0);
  const hovered = layer.analysis?.candidates.find((candidate) => same(candidate.point, layer.hover));
  for (const stone of variation.stones) {
    const { x, y } = pointToPixel(geometry, stone.point);
    const ink = stone.color === "Black" ? "#fff" : "#000";
    if (stone.move_number === 1 && hovered && same(stone.point, layer.hover)) {
      // The hovered move keeps its numbers inside a red ring instead of a "1".
      context.strokeStyle = "rgb(255, 0, 0)";
      context.lineWidth = (r + 1) / 11;
      disc(context, x, y, r + 1);
      context.stroke();
      context.fillStyle = ink;
      suggestionRows(context, geometry, x, y, hovered, board.to_play);
      continue;
    }
    if (stone.move_number === last) {
      const far = (r * 16) / 15;
      const near = (r * 4) / 11;
      context.fillStyle = "rgb(255, 0, 0)";
      context.beginPath();
      context.moveTo(x - far, y - far);
      context.lineTo(x - far, y - near);
      context.lineTo(x - near, y - far);
      context.closePath();
      context.fill();
    }
    context.fillStyle = ink;
    text(context, String(stone.move_number), x, y, UI_FONT, r * 1.4, stone.move_number >= 100 ? r * 1.85 : r * 1.4);
  }
}

/** `EmptyBoardHint`: a dark pill in the middle of an empty board. */
function drawEmptyHint(context: Context, geometry: Geometry, hint: string, dpr: number) {
  const { boardX, boardY, boardWidth, boardHeight } = geometry;
  const size = Math.max(13 * dpr, boardWidth / 48);
  context.font = `${size}px ${UI_FONT}`;
  context.textBaseline = "alphabetic";
  context.textAlign = "center";
  const metrics = context.measureText(hint);
  const ascent = Math.round(metrics.fontBoundingBoxAscent);
  const lineHeight = ascent + Math.round(metrics.fontBoundingBoxDescent);
  const height = Math.trunc(lineHeight * 1.5);
  const padding = Math.trunc((lineHeight * 3) / 4);
  const width = Math.min(boardWidth - 20 * dpr, Math.ceil(metrics.width) + padding * 2);
  const left = boardX + Math.trunc((boardWidth - width) / 2);
  const top = boardY + Math.trunc((boardHeight - height) / 2);
  context.fillStyle = "rgba(20, 22, 26, 0.667)";
  context.beginPath();
  context.roundRect(left, top, width, height, height / 2);
  context.fill();
  context.fillStyle = "#f1f2f4";
  context.fillText(hint, left + width / 2, top + Math.trunc((height - lineHeight) / 2) + ascent, width - padding);
  context.textAlign = "left";
}
