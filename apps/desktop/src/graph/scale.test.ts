import { describe, expect, test } from "vitest";
import type { GraphPointView } from "@/api/backend";
import { graphView, midGameAnalysis } from "../../e2e/fixtures";
import {
  type GraphLayout,
  analysedRanges,
  areaPaths,
  indexAtX,
  linePaths,
  liveGraph,
  scoreRange,
  xForIndex,
  yForScore,
  yForWinrate,
} from "./scale";

const layout = (count: number): GraphLayout => ({ width: 640, height: 132, left: 40, right: 40, top: 22, bottom: 14, count });

function point(index: number, winrate: number | null, score = 0): GraphPointView {
  return {
    node_id: index,
    move_number: index,
    summary: winrate === null ? null : { winrate, score_lead: score, visits: 100 },
  };
}

describe("coordinate mapping", () => {
  test("the first and last points sit on the plot edges", () => {
    expect(xForIndex(layout(101), 0)).toBe(40);
    expect(xForIndex(layout(101), 100)).toBe(600);
    expect(xForIndex(layout(101), 50)).toBe(320);
  });

  test("winrate runs from the bottom (0%) to the top (100%)", () => {
    expect(yForWinrate(layout(10), 1)).toBe(22);
    expect(yForWinrate(layout(10), 0)).toBe(118);
    expect(yForWinrate(layout(10), 0.5)).toBe(70);
    expect(yForWinrate(layout(10), 1.4)).toBe(22);
  });

  test("score uses a symmetric secondary axis", () => {
    expect(yForScore(layout(10), 0, 10)).toBe(70);
    expect(yForScore(layout(10), 10, 10)).toBe(22);
    expect(yForScore(layout(10), -25, 10)).toBe(118);
  });

  test("score range doubles from 5 until every score fits", () => {
    expect(scoreRange([point(0, 0.5, 3)])).toBe(5);
    expect(scoreRange([point(0, 0.5, -12.5), point(1, null)])).toBe(20);
    expect(scoreRange([])).toBe(5);
  });
});

describe("jumping", () => {
  test.each([
    [40, 0],
    [0, 0],
    [600, 100],
    [900, 100],
    [320, 50],
    [322.7, 50],
    [323, 51],
  ])("x = %f jumps to point %i of 101", (x, index) => {
    expect(indexAtX(layout(101), x)).toBe(index);
  });

  test("a single-point line always maps to that point", () => {
    expect(indexAtX(layout(1), 500)).toBe(0);
  });

  test("every point is reachable by clicking on its own x", () => {
    const l = layout(237);
    for (let index = 0; index < 237; index++) expect(indexAtX(l, xForIndex(l, index))).toBe(index);
  });
});

describe("paths", () => {
  const points = [point(0, 0.5), point(1, 0.6), point(2, null), point(3, 0.4), point(4, 0.45)];

  test("lines break where analysis is missing", () => {
    const paths = linePaths(layout(5), points, (summary) => yForWinrate(layout(5), summary.winrate));
    expect(paths).toHaveLength(2);
    expect(paths[0]?.startsWith("M40 ")).toBe(true);
  });

  test("areas close at the bottom edge for each analysed run", () => {
    const areas = areaPaths(layout(5), points);
    expect(areas).toHaveLength(2);
    expect(areas.every((area) => area.endsWith("Z"))).toBe(true);
  });

  test("coverage lists contiguous analysed runs", () => {
    expect(analysedRanges(points)).toEqual([
      [0, 1],
      [3, 4],
    ]);
    expect(analysedRanges([point(0, null)])).toEqual([]);
  });

  test("the current node shows the summary carried by the newest analysis", () => {
    const graph = graphView(21, 20, {});
    const analysis = midGameAnalysis(7, 20);
    const live = liveGraph(graph, analysis);
    expect(live[20]?.summary).toEqual(analysis.summary);
    expect(live[19]?.summary).toBeNull();
    expect(liveGraph(graph, null)).toBe(graph.points);
  });
});
