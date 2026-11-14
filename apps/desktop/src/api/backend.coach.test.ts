import { beforeEach, expect, test, vi } from "vitest";
import { defaultCoachSetup } from "../../e2e/fixtures";
import type { CoachView, CommandError, WireAppEvent } from "./backend";

interface FakeChannel { onmessage: (event: WireAppEvent) => void }
const ipc = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as FakeChannel[] }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class implements FakeChannel {
  onmessage: (event: WireAppEvent) => void = () => {};
  constructor() { ipc.channels.push(this); }
} }));
beforeEach(() => { vi.resetModules(); ipc.invoke.mockReset(); ipc.channels.length = 0; });

test("coach and model commands forward typed inputs without implementing coaching", async () => {
  const api = await import("./backend");
  const cases: [() => Promise<unknown>, string, object | undefined][] = [
    [() => api.startCoach(defaultCoachSetup), "start_coach", { setup: defaultCoachSetup }],
    [api.coachView, "coach_view", undefined], [api.coachHint, "coach_hint", undefined],
    [() => api.coachDecision("Retry"), "coach_decision", { decision: "Retry" }],
    [() => api.coachDecision("ShowBest"), "coach_decision", { decision: "ShowBest" }],
    [() => api.coachDecision("Continue"), "coach_decision", { decision: "Continue" }],
    [api.coachReview, "coach_review", undefined],
    [api.humanslModelStatus, "humansl_model_status", undefined],
    [() => api.importHumanslModel("/models/human.bin.gz"), "import_humansl_model", { path: "/models/human.bin.gz" }],
    [api.pauseAssetDownload, "pause_asset_download", undefined],
  ];
  for (const [call, name, args] of cases) {
    await call(); expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [name, args] : [name]));
  }
  const error = { type: "Coach", data: { type: "TensorRtUnsupported" } };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.startCoach({})).rejects.toBe(error);
});

test("coaching events share the ordered channel and stay out of legacy listeners", async () => {
  const api = await import("./backend"); ipc.invoke.mockResolvedValue(73);
  const old = vi.fn(); const coach = vi.fn(); const rating = vi.fn(); const summary = vi.fn();
  const ids = await Promise.all([
    api.subscribe(old), api.subscribeCoachUpdated(coach), api.subscribeCoachMoveRated(rating), api.subscribeCoachSummary(summary),
  ]);
  expect(ipc.channels).toHaveLength(1);
  ipc.channels[0]!.onmessage({ type: "CoachUpdated", data: { hints: 1 } });
  ipc.channels[0]!.onmessage({ type: "CoachMoveRated", data: { verdict: "Blunder", node_id: 8 } });
  ipc.channels[0]!.onmessage({ type: "CoachSummary", data: { total_moves: 17, result: "B+R" } });
  expect(old).not.toHaveBeenCalled(); expect(coach).toHaveBeenCalledWith({ hints: 1 });
  expect(rating).toHaveBeenCalledWith({ verdict: "Blunder", node_id: 8 });
  expect(summary).toHaveBeenCalledWith({ total_moves: 17, result: "B+R" });
  for (const id of ids) await api.unsubscribe(id);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 73 });
});

test("coach IPC and scripted mock preserve human model load failure reasons", async () => {
  const api = await import("./backend");
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const error: CommandError = {
    type: "Coach", data: { type: "HumanModelLoadFailed", data: { reason: "injected human model load failure" } },
  };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.startCoach(defaultCoachSetup)).rejects.toBe(error);
  expect(ipc.invoke).toHaveBeenLastCalledWith("start_coach", { setup: defaultCoachSetup });
  mock.setScript({ commands: [
    { command: "startCoach", result: {}, error },
    { command: "coachView", result: null },
  ] });
  await expect(mock.startCoach(defaultCoachSetup)).rejects.toEqual(error);
  expect(await mock.coachView()).toBeNull();
  expect(mock.recordedCalls()).toEqual([
    { command: "startCoach", args: [defaultCoachSetup] }, { command: "coachView", args: [] },
  ]);
});

test("coach mock replays decisions, hints, summaries and model states with DTO defaults", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "startCoach", result: { setup: { mode: "LiveFeedback" }, hints: 1 }, events: [
      { type: "CoachUpdated", data: { awaiting_decision: true } },
      { type: "CoachMoveRated", data: { verdict: "Blunder", points_lost: 22 } },
      { type: "CoachSummary", data: { take_backs: 2, worst_moves: [{ node_id: 5, points_lost: 22 }] } },
    ] },
    { command: "coachView", result: { take_backs: 2 } },
    { command: "coachHint", result: { type: "Reading" } },
    { command: "coachHint", result: { type: "Region", data: { column: 2 } } },
    { command: "coachHint", result: { type: "Exact", data: { mv: { type: "Play", data: { color: "White", point: { x: 2, y: 3 } } } } } },
    { command: "coachDecision", result: { type: "Reading" } },
    { command: "coachDecision", result: { type: "Exact", data: { human_share: 0.02 } } },
    { command: "coachDecision", result: { type: "Reading" } },
    { command: "coachReview", result: undefined },
    { command: "humanslModelStatus", result: { type: "Missing" } },
    { command: "startAssetDownload", result: { phase: "Downloading", asset_id: "human-sl" } },
    { command: "pauseAssetDownload", result: { phase: "Paused", downloaded_bytes: 50 } },
    { command: "humanslModelStatus", result: { type: "Downloading", data: { downloaded_bytes: 50, paused: true } } },
    { command: "humanslModelStatus", result: { type: "Corrupt" } },
    { command: "importHumanslModel", result: { type: "Ready", data: { path: "/models/human.bin.gz" } } },
  ] });
  const old = vi.fn(); const coach = vi.fn(); const rating = vi.fn(); const summary = vi.fn();
  const ids = await Promise.all([
    mock.subscribe(old), mock.subscribeCoachUpdated(coach), mock.subscribeCoachMoveRated(rating), mock.subscribeCoachSummary(summary),
  ]);
  const view: CoachView = await mock.startCoach({});
  expect(view.setup?.rank).toEqual({ type: "Dan", data: 3 });
  expect(view.hints).toBe(1); expect(view.take_backs).toBe(0);
  expect(coach.mock.calls[0]?.[0].awaiting_decision).toBe(true);
  expect(rating.mock.calls[0]?.[0].human_share).toBeNull();
  expect(summary.mock.calls[0]?.[0].take_backs).toBe(2);
  expect(summary.mock.calls[0]?.[0].worst_moves[0].points_lost).toBe(22);
  expect(old).not.toHaveBeenCalled();
  expect((await mock.coachView())?.take_backs).toBe(2);
  expect((await mock.coachHint()).type).toBe("Reading");
  const region = await mock.coachHint();
  expect(region.type === "Region" && region.data.row).toBe(0);
  expect(region.type === "Region" && region.data.column).toBe(2);
  const exact = await mock.coachHint();
  expect(exact.type === "Exact" && exact.data.human_share).toBeNull();
  await mock.coachDecision("Retry"); await mock.coachDecision("ShowBest"); await mock.coachDecision("Continue");
  await mock.coachReview(); expect(await mock.humanslModelStatus()).toEqual({ type: "Missing" });
  await mock.startAssetDownload("human-sl");
  expect((await mock.pauseAssetDownload()).phase).toBe("Paused");
  const progress = await mock.humanslModelStatus();
  expect(progress.type === "Downloading" && progress.data.total_bytes).toBe(99066230);
  expect(await mock.humanslModelStatus()).toEqual({ type: "Corrupt" });
  expect(await mock.importHumanslModel("/models/human.bin.gz")).toEqual({ type: "Ready", data: { path: "/models/human.bin.gz" } });
  for (const call of [() => mock.startCoach({}), mock.coachView, mock.humanslModelStatus, mock.coachHint, () => mock.coachDecision("Retry"), mock.coachReview, () => mock.importHumanslModel("x"), mock.pauseAssetDownload]) {
    await expect(call()).rejects.toThrow("Expected mock command");
  }
  for (const id of ids) await mock.unsubscribe(id);
});

test("mock fills coaching setup and human model defaults while preserving explicit values", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "settings", result: { version: 1, engine: { program: "e", model: "m", config: "c", extra_args: [] }, pondering: true } },
    { command: "settings", result: { version: 1, engine: null, pondering: true, coach_setup: { mode: "ReviewAfter", handicap: 4, auto_save: false }, game_setup: { white: { type: "Ai", data: { type: "HumanSl", data: { pace: "Fast" } } } } } },
  ] });
  const defaults = await mock.settings();
  expect(defaults.coach_setup).toEqual(defaultCoachSetup); expect(defaults.engine?.human_model).toBeUndefined();
  const explicit = await mock.settings();
  expect(explicit.coach_setup?.mode).toBe("ReviewAfter"); expect(explicit.coach_setup?.auto_save).toBe(false);
  expect(explicit.coach_setup?.handicap).toBe(4); expect(explicit.coach_setup?.from_current_position).toBe(false);
  const ai = explicit.game_setup?.white;
  expect(ai?.type === "Ai" && ai.data.type === "HumanSl" && ai.data.data.pace).toBe("Fast");
  expect(ai?.type === "Ai" && ai.data.type === "HumanSl" && ai.data.data.rank).toEqual({ type: "Dan", data: 3 });
});
