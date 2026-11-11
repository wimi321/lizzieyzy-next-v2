import type { AnalysisView, CandidateView, MoveRank } from "@/api/backend";

/** LizzieYzy Next's `limit-max-suggestion` and friends, ported from `BoardRenderer`. */

export interface Rgba {
  r: number;
  g: number;
  b: number;
  /** 0..255, as `java.awt.Color`. */
  a: number;
}

export function css({ r, g, b, a }: Rgba): string {
  return `rgba(${r}, ${g}, ${b}, ${a / 255})`;
}

/** `java.awt.Color.HSBtoRGB`. */
export function hsb(hue: number, saturation: number, brightness: number): [number, number, number] {
  const h = (hue - Math.floor(hue)) * 6;
  const f = h - Math.floor(h);
  const p = brightness * (1 - saturation);
  const q = brightness * (1 - saturation * f);
  const t = brightness * (1 - saturation * (1 - f));
  const [r, g, b] = (
    [
      [brightness, t, p],
      [q, brightness, p],
      [p, brightness, t],
      [p, q, brightness],
      [t, p, brightness],
      [brightness, p, q],
    ] as const
  )[Math.floor(h) % 6]!;
  const byte = (value: number) => Math.trunc(value * 255 + 0.5);
  return [byte(r), byte(g), byte(b)];
}

/**
 * A suggestion's fill. The best move is cyan; the others run from red to green with their share
 * of the best move's visits, not with the winrate they give up. Opacity follows the same share.
 */
export function suggestionColor(visits: number, maxVisits: number, best: boolean): Rgba {
  const ratio = maxVisits > 0 ? Math.min(1, visits / maxVisits) : 0;
  const hue = best ? 0.5 : Math.sqrt(ratio) / 3;
  const [r, g, b] = hsb(hue, 1, 0.85);
  return { r, g, b, a: Math.trunc(32 + 208 * suggestionAlphaRatio(ratio)) };
}

/** 0 for a move with under 1/150 of the best move's visits, 1 for the best move. */
export function suggestionAlphaRatio(ratio: number): number {
  if (ratio <= 0) return 0;
  return Math.min(1, Math.max(0, Math.log(ratio) / 5 + 1));
}

/** The moves that get a circle: engine order, the first `limit`, searched ones only. */
export function shownSuggestions(analysis: AnalysisView, limit: number): CandidateView[] {
  return [...analysis.candidates]
    .sort((a, b) => a.order - b.order)
    .slice(0, Math.max(0, limit))
    .filter((candidate) => candidate.point !== null && candidate.visits > 0);
}

/** Java's `Utils.getPlayoutsString`: 999, 1.0k, 9.9k, 10k, 12.3m. */
export function playoutsText(visits: number): string {
  if (visits >= 10_000_000) return `${(Math.round(visits / 100_000) / 10).toFixed(1)}m`;
  if (visits >= 9950) return `${Math.round(visits / 1000)}k`;
  if (visits >= 1000) return `${(Math.round(visits / 100) / 10).toFixed(1)}k`;
  return String(Math.max(0, Math.round(visits)));
}

/** One decimal, no plus sign, as `String.valueOf(double)` prints a rounded score. */
export function scoreText(score: number): string {
  const rounded = Math.round(score * 10) / 10;
  return rounded === 0 ? "0.0" : rounded.toFixed(1);
}

export function winrateText(winrate: number): string {
  return (Math.min(1, Math.max(0, winrate)) * 100).toFixed(1);
}

/**
 * `BoardRenderer.drawString`'s font size: as wide as allowed, capped by the height.
 * `widthAt100` is the string's width at 100 px.
 */
export function fitFontSize(widthAt100: number, maximumHeight: number, maximumWidth: number): number {
  if (widthAt100 <= 0) return maximumHeight;
  return Math.max(1, Math.min(maximumHeight, Math.round((100 * maximumWidth) / widthAt100)));
}

/** `MoveRankDefinition`'s colours for the mark on the last move. */
export const RANK_COLORS: Record<MoveRank, string> = {
  Best: "rgb(0, 180, 0)",
  Good: "rgb(140, 202, 34)",
  Normal: "rgb(180, 180, 0)",
  Inaccuracy: "rgb(200, 140, 50)",
  Mistake: "rgb(208, 16, 19)",
  Blunder: "rgb(155, 25, 150)",
};

export type BlunderLevel = "bad" | "doubtful" | "good";

/** How a played move compares with the engine's best: winrate in points, score in stones. */
export function blunderLevel(winrateDrop: number, scoreDrop: number): BlunderLevel {
  if (winrateDrop >= 10 || scoreDrop >= 5) return "bad";
  if (winrateDrop >= 1.5 || scoreDrop >= 1) return "doubtful";
  return "good";
}

/** Java's star points: the 3-3 points and centre on 9x9, 4-4 points on larger boards. */
export function starRadius(boardWidth: number, boardHeight: number): number {
  return Math.trunc(Math.trunc(0.015 * Math.min(boardWidth, boardHeight)) / 2);
}

export function lineWidths(boardWidth: number, available: number): { border: number; inner: number } {
  return {
    border: Math.max(boardWidth > 560 ? 2 : 1, available / 481),
    inner: Math.max(1, available / 750),
  };
}

/**
 * The whole pixels a grid line of this width covers, as an offset from the line's own pixel and
 * a count. Java strokes the grid without antialiasing, which is what keeps it crisp: a 2 px line
 * takes its pixel and the one before, a 3 px line one on either side.
 */
export function lineSpan(width: number): [offset: number, size: number] {
  const first = Math.ceil(-0.25 - width / 2) || 0;
  const last = Math.ceil(-0.25 + width / 2) - 1;
  return [first, Math.max(1, last - first + 1)];
}
