import type { AnalysisSummary, AnalysisView, GraphPointView, GraphView } from "@/api/backend";

/** Plot area inside the SVG, in CSS pixels. */
export interface GraphLayout {
  width: number;
  height: number;
  left: number;
  right: number;
  top: number;
  bottom: number;
  /** Points on the displayed line. */
  count: number;
}

export function plotWidth(layout: GraphLayout): number {
  return Math.max(0, layout.width - layout.left - layout.right);
}

export function plotHeight(layout: GraphLayout): number {
  return Math.max(0, layout.height - layout.top - layout.bottom);
}

export function xForIndex(layout: GraphLayout, index: number): number {
  if (layout.count <= 1) return layout.left;
  return layout.left + (index / (layout.count - 1)) * plotWidth(layout);
}

/** The point nearest to a CSS x position, clamped to the line. Drives click-and-drag jumps. */
export function indexAtX(layout: GraphLayout, x: number): number {
  if (layout.count <= 1 || plotWidth(layout) === 0) return 0;
  const ratio = (x - layout.left) / plotWidth(layout);
  return Math.min(layout.count - 1, Math.max(0, Math.round(ratio * (layout.count - 1))));
}

/** Black's winrate: 100% at the top, 0% at the bottom. */
export function yForWinrate(layout: GraphLayout, winrate: number): number {
  const clamped = Math.min(1, Math.max(0, winrate));
  return layout.top + (1 - clamped) * plotHeight(layout);
}

/** Score lead on a symmetric secondary axis: +range at the top, 0 in the middle. */
export function yForScore(layout: GraphLayout, score: number, range: number): number {
  const clamped = Math.min(range, Math.max(-range, score));
  return layout.top + (0.5 - clamped / (2 * range)) * plotHeight(layout);
}

/** Smallest of 5, 10, 20, 40… points that holds every score on the line. */
export function scoreRange(points: readonly GraphPointView[]): number {
  const largest = points.reduce((max, point) => Math.max(max, Math.abs(point.summary?.score_lead ?? 0)), 0);
  let range = 5;
  while (range < largest) range *= 2;
  return range;
}

/** The current node shows its newest summary before the next DocumentChanged arrives. */
export function liveGraph(graph: GraphView, analysis: AnalysisView | null): GraphPointView[] {
  if (!analysis) return graph.points;
  return graph.points.map((point) =>
    point.node_id === analysis.node_id ? { ...point, summary: analysis.summary } : point,
  );
}

/** SVG path data for each run of analysed points; gaps stay open. */
export function linePaths(
  layout: GraphLayout,
  points: readonly GraphPointView[],
  y: (summary: AnalysisSummary) => number,
): string[] {
  const paths: string[] = [];
  let current = "";
  points.forEach((point, index) => {
    if (!point.summary) {
      if (current) paths.push(current);
      current = "";
      return;
    }
    const command = current ? "L" : "M";
    current += `${command}${round(xForIndex(layout, index))} ${round(y(point.summary))}`;
  });
  if (current) paths.push(current);
  return paths;
}

/** Closed areas between each run of the winrate line and the bottom edge. */
export function areaPaths(layout: GraphLayout, points: readonly GraphPointView[]): string[] {
  const bottom = round(layout.top + plotHeight(layout));
  const areas: string[] = [];
  let run: number[] = [];
  const flush = () => {
    if (run.length === 0) return;
    const first = run[0] ?? 0;
    const last = run[run.length - 1] ?? first;
    const top = run
      .map((index) => `${round(xForIndex(layout, index))} ${round(yForWinrate(layout, points[index]?.summary?.winrate ?? 0.5))}`)
      .join(" L");
    areas.push(`M${round(xForIndex(layout, first))} ${bottom} L${top} L${round(xForIndex(layout, last))} ${bottom} Z`);
    run = [];
  };
  points.forEach((point, index) => {
    if (point.summary) run.push(index);
    else flush();
  });
  flush();
  return areas;
}

/** Index ranges that have analysis, for the coverage strip under the plot. */
export function analysedRanges(points: readonly GraphPointView[]): Array<[number, number]> {
  const ranges: Array<[number, number]> = [];
  points.forEach((point, index) => {
    if (!point.summary) return;
    const last = ranges[ranges.length - 1];
    if (last && last[1] === index - 1) last[1] = index;
    else ranges.push([index, index]);
  });
  return ranges;
}

/** Analysed points with no analysed neighbour: a run of one draws no line or area, so it gets a dot. */
export function isolatedIndices(points: readonly GraphPointView[]): number[] {
  return analysedRanges(points)
    .filter(([from, to]) => from === to)
    .map(([index]) => index);
}

function round(value: number): number {
  return Math.round(value * 10) / 10;
}
