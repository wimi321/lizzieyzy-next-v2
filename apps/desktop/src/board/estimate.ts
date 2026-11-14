import type { BoardView, CandidateView, EstimateStyle } from "@/api/backend";
import { type Context, NUMBER_FONT, text } from "./canvas";
import { type Geometry, pointToPixel } from "./geometry";

/** Kata形势: ownership in Black's perspective (+1 Black, -1 White) and how to draw it. */
export interface Estimate {
  style: EstimateStyle;
  values: readonly number[];
}

/** 以棋子下大面积方块表示占有率 sits under the stones; the other styles over them. */
export const underStones = (estimate: Estimate | null): boolean => estimate?.style === "BigSquare";

function centre(geometry: Geometry, board: Pick<BoardView, "width">, index: number) {
  const point = pointToPixel(geometry, { x: index % board.width, y: Math.floor(index / board.width) });
  return { x: point.x - 0.5, y: point.y - 0.5 };
}

/**
 * Java's `drawKataEstimateByTransparent` and `drawKataEstimateBySize`, from Black's side: a square
 * whose darkness, lightness or size follows how surely each point belongs to its owner.
 */
export function drawEstimate(context: Context, geometry: Geometry, board: Pick<BoardView, "width" | "stones">, estimate: Estimate) {
  const { cell } = geometry;
  const { style } = estimate;
  estimate.values.forEach((value, index) => {
    if (value === 0 || !Number.isFinite(value)) return;
    const black = value > 0;
    const strength = Math.min(1, Math.abs(value));
    const stone = board.stones[index] ?? null;
    const { x, y } = centre(geometry, board, index);

    if (style === "BySize") {
      // Java leaves points under 20% blank and scales the rest to 60% of a cell at most.
      if (strength <= 0.2) return;
      const length = Math.trunc(strength * 0.6 * cell);
      if (length <= 0) return;
      context.fillStyle = black ? "rgba(0, 0, 0, 0.706)" : "rgba(255, 255, 255, 0.706)";
      context.fillRect(x - length / 2, y - length / 2, length, length);
      return;
    }
    if (style === "BigSquare") {
      context.fillStyle = black ? `rgba(0, 0, 0, ${(strength * 105) / 255})` : `rgba(255, 255, 255, ${(strength * 165) / 255})`;
      context.fillRect(x - cell / 2, y - cell / 2, cell, cell);
      return;
    }
    // 不显示在活子上: a stone of the owner's colour is alive; only dead stones keep a square.
    if (style === "TransparentOffStones" && stone === (black ? "Black" : "White")) return;
    const alpha = Math.trunc(strength * 255);
    if (alpha <= 0) return;
    const half = style === "TransparentSmall" ? Math.round(cell / (black ? 8 : 6)) : Math.trunc(cell / 4);
    if (black) {
      // On a black stone a translucent black square would vanish: Java paints it grey instead.
      const gray = 127 - Math.trunc((alpha - 1) / 2);
      context.fillStyle = stone === "Black" ? `rgb(${gray}, ${gray}, ${gray})` : `rgba(0, 0, 0, ${alpha / 255})`;
    } else {
      context.fillStyle = `rgba(255, 255, 255, ${alpha / 255})`;
    }
    context.fillRect(x - half, y - half, half * 2, half * 2);
  });
}

/** Cold to hot: blue for a faint first instinct, through green and yellow, to red for the strongest. */
function heatColor(share: number, alpha: number): string {
  const hue = Math.round(220 * (1 - share));
  return `hsla(${hue}, 85%, 52%, ${alpha})`;
}

/**
 * 纯网络 and 策略网络: the policy network's first instinct for the side to move, as a disc per
 * point in heat colours, with the percentage on points that reach 1%.
 */
export function drawPolicy(context: Context, geometry: Geometry, board: Pick<BoardView, "width" | "height" | "stones">, policy: readonly number[]) {
  const points = board.width * board.height;
  let max = 0;
  for (let index = 0; index < points; index += 1) max = Math.max(max, policy[index] ?? 0);
  if (max <= 0) return;
  const { cell } = geometry;
  for (let index = 0; index < points; index += 1) {
    const value = policy[index] ?? -1;
    if (value <= 0 || board.stones[index]) continue;
    const share = Math.sqrt(value / max);
    if (value < 0.001 && share < 0.05) continue;
    const { x, y } = centre(geometry, board, index);
    context.fillStyle = heatColor(share, 0.25 + 0.6 * share);
    context.beginPath();
    context.arc(x, y, cell * (0.18 + 0.27 * share), 0, Math.PI * 2);
    context.fill();
    if (value >= 0.01) {
      const label = value >= 0.995 ? "100" : String(Math.round(value * 100));
      context.fillStyle = share > 0.55 ? "#ffffff" : "#10131a";
      text(context, label, x, y, NUMBER_FONT, cell * 0.36, cell * 0.7);
    }
  }
}

/**
 * 小棋盘 → 热点图: Java's soft red spots (`assets/heatmap.png`), sized by each candidate's first instinct (第一感) or by its
 * share of the search (计算后), the largest about 2.7 cells across.
 */
export function drawHeat(context: Context, geometry: Geometry, board: Pick<BoardView, "width">, candidates: readonly CandidateView[], kind: "Policy" | "AfterSearch") {
  const value = (candidate: CandidateView) => (kind === "Policy" ? candidate.prior : candidate.visits);
  const max = candidates.reduce((best, candidate) => Math.max(best, value(candidate)), 0);
  if (max <= 0) return;
  for (const candidate of candidates) {
    if (!candidate.point || value(candidate) <= 0) continue;
    const radius = (value(candidate) / max) * 1.35 * geometry.cell;
    if (radius < 1) continue;
    const { x, y } = centre(geometry, board, candidate.point.y * board.width + candidate.point.x);
    const gradient = context.createRadialGradient(x, y, 0, x, y, radius);
    // Java's heatmap.png: pure red, fading out by two thirds of the radius.
    for (const [stop, alpha] of [[0, 1], [0.15, 0.84], [0.3, 0.6], [0.45, 0.34], [0.6, 0.11], [0.67, 0]] as const) {
      gradient.addColorStop(stop, `rgba(255, 0, 0, ${alpha})`);
    }
    context.fillStyle = gradient;
    context.beginPath();
    context.arc(x, y, radius, 0, Math.PI * 2);
    context.fill();
  }
}
