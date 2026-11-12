import { beforeEach, expect, test, vi } from "vitest";
import { boardView, documentView } from "../../e2e/fixtures";
import type { AnalysisView } from "./backend";

beforeEach(() => vi.resetModules());

const saved: AnalysisView = {
  node_id: 0, revision: 1, winrate: 0.6, score_lead: 2.5, visits: 12345,
  summary: { winrate: 0.6, score_lead: 2.5, visits: 12345 },
  candidates: [{
    coordinate: "C3", point: { x: 2, y: 2 }, visits: 12000,
    winrate: 0.6, score_lead: 2.5, prior: 0.2, order: 0, pv: [],
  }],
  ownership: null, is_during_search: false, source: "Saved",
};

test("mock keeps saved provenance and supplies score defaults to old events and queries", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const view = documentView(boardView(5, 5, []), 0);
  view.graph.points[0]!.summary = saved.summary;
  mock.setScript({
    initialEvents: [
      { type: "DocumentChanged", data: { revision: 1, view } },
      { type: "AnalysisUpdated", data: saved },
    ],
    commands: [{ command: "wholeGameResult", result: saved }],
  });
  const listener = vi.fn();
  const subscription = await mock.subscribe(listener);
  try {
    const graph = listener.mock.calls[0]![0].data.view.graph;
    expect(graph.points[0].summary.score_lead_available).toBe(true);
    const analysis = listener.mock.calls[1]![0].data as AnalysisView;
    expect(analysis.source).toBe("Saved");
    expect(analysis.visits_per_second).toBeNull();
    expect(analysis.summary.score_lead_available).toBe(true);
    expect(analysis.candidates[0]!.score_lead_available).toBe(true);
    const result = await mock.wholeGameResult(0);
    expect(result!.summary.score_lead_available).toBe(true);
    expect(saved.summary.score_lead_available).toBeUndefined();
  } finally {
    await mock.unsubscribe(subscription);
  }
});

test("mock preserves an explicit missing score in imported legacy data", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const legacy: AnalysisView = {
    ...saved,
    summary: { ...saved.summary, score_lead_available: false },
    candidates: [{ ...saved.candidates[0]!, score_lead_available: false }],
  };
  mock.setScript({ commands: [{ command: "wholeGameResult", result: legacy }] });
  const result = await mock.wholeGameResult(0);
  expect(result!.summary.score_lead_available).toBe(false);
  expect(result!.candidates[0]!.score_lead_available).toBe(false);
});
