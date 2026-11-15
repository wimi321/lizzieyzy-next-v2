import { beforeEach, expect, test, vi } from "vitest";
import { boardView, documentView } from "../../e2e/fixtures";
import type { AppEvent, AnalysisToolsCommands, ScoringView } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));
beforeEach(() => { vi.resetModules(); ipc.invoke.mockReset(); });
const point = { x: 2, y: 3 };
const params = { wide_root_noise: 0.4 };
const score: ScoringView = {
  node_id: 0, rule: "Area", dead_stones: [], neutral_points: [], ownership: [],
  black: { living_stones: 0, territory: 0, dead_stones: 0, captures: 0, komi: 0, handicap_compensation: 0, total: 0 },
  white: { living_stones: 0, territory: 0, dead_stones: 0, captures: 0, komi: 0, handicap_compensation: 0, total: 0 },
  score_lead: 0, result: "0",
};
function calls(backend: Required<AnalysisToolsCommands>) {
  return [
    { run: () => backend.setAnalysisRestriction("Allow", [point]), wire: "set_analysis_restriction", args: { kind: "Allow", points: [point] } },
    { run: () => backend.addRestrictionPoints("Avoid", [point]), wire: "add_restriction_points", args: { kind: "Avoid", points: [point] } },
    { run: () => backend.removeRestrictionPoints([point]), wire: "remove_restriction_points", args: { points: [point] } },
    { run: () => backend.clearAnalysisRestriction(), wire: "clear_analysis_restriction" },
    { run: () => backend.setRestrictionDepth("Allow", { type: "WholeSearch" }), wire: "set_restriction_depth", args: { kind: "Allow", depth: { type: "WholeSearch" } } },
    { run: () => backend.searchParams(), wire: "search_params" },
    { run: () => backend.setSearchParams(params), wire: "set_search_params", args: { params } },
    { run: () => backend.continueLadder(), wire: "continue_ladder" },
    { run: () => backend.startScoring(), wire: "start_scoring" },
    { run: () => backend.toggleScoringPoint(point), wire: "toggle_scoring_point", args: { point } },
    { run: () => backend.setScoringRule("Territory"), wire: "set_scoring_rule", args: { rule: "Territory" } },
    { run: () => backend.commitScoring(), wire: "commit_scoring" },
    { run: () => backend.cancelScoring(), wire: "cancel_scoring" },
  ];
}

test("research APIs preserve arguments, replies and typed failures", async () => {
  const backend = await import("./backend");
  for (const call of calls(backend)) {
    ipc.invoke.mockResolvedValueOnce(score);
    expect(await call.run()).toEqual(score);
    expect(ipc.invoke.mock.calls.at(-1)).toEqual(call.args ? [call.wire, call.args] : [call.wire]);
    const error = { type: "AnalysisTools", data: { type: "GameActive" } };
    ipc.invoke.mockRejectedValueOnce(error);
    await expect(call.run()).rejects.toEqual(error);
  }
});

test("every new mock entry requires a command script, including reads", async () => {
  const mock = await import("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "setAnalysisRestriction", result: null },
    { command: "addRestrictionPoints", result: null },
    { command: "removeRestrictionPoints", result: null },
    { command: "clearAnalysisRestriction", result: undefined },
    { command: "setRestrictionDepth", result: undefined },
    { command: "searchParams", result: params },
    { command: "setSearchParams", result: params },
    { command: "continueLadder", result: { outcome: "Escaped", moves: 3 } },
    { command: "startScoring", result: score },
    { command: "toggleScoringPoint", result: score },
    { command: "setScoringRule", result: score },
    { command: "commitScoring", result: 8 },
    { command: "cancelScoring", result: undefined },
  ] });
  for (const call of calls(mock)) await call.run();
  expect(mock.recordedCalls()).toHaveLength(13);
  for (const call of calls(mock)) await expect(call.run()).rejects.toThrow("Expected mock command (none)");
});

test("legacy mock documents default research fields without replacing explicit values", async () => {
  const mock = await import("../../e2e/mock-backend");
  const view = documentView(boardView(9, 9, []), 0);
  mock.setScript({ initialEvents: [{ type: "DocumentChanged", data: { revision: 1, view } }] });
  const listener = vi.fn();
  const id = await mock.subscribe(listener);
  try {
    const event = listener.mock.calls[0]![0] as AppEvent;
    if (event.type !== "DocumentChanged") throw new Error("expected document");
    expect(event.data.view.analysis_restriction).toBeNull();
    expect(event.data.view.restricted_analysis).toBeNull();
    expect(event.data.view.scoring).toBeNull();
    mock.pushEvents([{ type: "DocumentChanged", data: { revision: 2, view: { ...view, scoring: score } } }]);
    expect(listener.mock.calls.at(-1)![0].data.view.scoring).toEqual(score);
  } finally { await mock.unsubscribe(id); }
});
