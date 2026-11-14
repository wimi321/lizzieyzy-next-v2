import { beforeEach, expect, test, vi } from "vitest";
import { defaultCommentarySettings, defaultCommentaryUpdated } from "../../e2e/fixtures";
import type { WireAppEvent } from "./backend";

interface FakeChannel { onmessage: (event: WireAppEvent) => void }
const ipc = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as FakeChannel[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: ipc.invoke,
  Channel: class implements FakeChannel {
    onmessage: (event: WireAppEvent) => void = () => {};
    constructor() { ipc.channels.push(this); }
  },
}));
beforeEach(() => { vi.resetModules(); ipc.invoke.mockReset(); ipc.channels.length = 0; });

test("commentary commands forward typed requests, key intent and failures unchanged", async () => {
  const api = await import("./backend");
  const request = { mode: { type: "Range" as const, data: { from: 3, to: 12 } }, follow_up: null };
  const cases: [() => Promise<unknown>, string, object | undefined][] = [
    [() => api.startCommentary(request), "start_commentary", { request }],
    [api.stopCommentary, "stop_commentary", undefined],
    [api.commentaryStatus, "commentary_status", undefined],
    [api.commentaryModels, "commentary_models", undefined],
    [() => api.setCommentaryKey("synthetic-test-key", false), "set_commentary_key", { key: "synthetic-test-key", remember: false }],
    [api.clearCommentaryKey, "clear_commentary_key", undefined],
    [api.commentaryKeyStatus, "commentary_key_status", undefined],
    [() => api.setCommentarySettings(defaultCommentarySettings), "set_commentary_settings", { settings: defaultCommentarySettings }],
  ];
  for (const [call, name, args] of cases) {
    await call();
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [name, args] : [name]));
  }
  const error = { type: "Commentary", data: { type: "NoAnalysis" } };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.startCommentary({ mode: { type: "NextMove" } })).rejects.toBe(error);
});

test("streaming, replacement and terminal snapshots share one ordered channel", async () => {
  const api = await import("./backend");
  ipc.invoke.mockResolvedValue(91);
  const legacy = vi.fn(); const commentary = vi.fn();
  const ids = await Promise.all([api.subscribe(legacy), api.subscribeCommentaryUpdated(commentary)]);
  expect(ipc.channels).toHaveLength(1);
  const updates: WireAppEvent[] = [
    { type: "CommentaryUpdated", data: { request_id: 1, phase: "Generating", delta: "先", text: "先" } },
    { type: "CommentaryUpdated", data: { request_id: 1, phase: "Generating", delta: "连接", text: "先连接" } },
    { type: "CommentaryUpdated", data: { request_id: 1, phase: "Generating", replacement: true, text: "先照顾弱子" } },
    { type: "CommentaryUpdated", data: { request_id: 1, phase: "Completed", issues: ["unverified coordinate"], text: "先照顾弱子" } },
  ];
  for (const event of updates) ipc.channels[0]!.onmessage(event);
  expect(commentary.mock.calls.map(([view]) => view)).toEqual(updates.map((event) => event.data));
  expect(legacy).not.toHaveBeenCalled();
  for (const id of ids) await api.unsubscribe(id);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 91 });
});

test("commentary mock fills optional snapshots and preserves scripts without doing service work", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "startCommentary", result: { request_id: 7, phase: "Preparing" }, events: [
      { type: "CommentaryUpdated", data: { request_id: 7, phase: "Generating", delta: "连接", text: "连接", positions: [{ node_id: 4 }] } },
    ] },
    { command: "commentaryStatus", result: { request_id: 7, phase: "Verifying", text: "连接" } },
    { command: "stopCommentary", result: { request_id: 7, phase: "Stopped" } },
    { command: "commentaryModels", result: ["local-model"] },
    { command: "setCommentaryKey", result: { has_key: true, stored: true, available: true } },
    { command: "commentaryKeyStatus", result: {} },
    { command: "clearCommentaryKey", result: {} },
    { command: "setCommentarySettings", result: { model: "local-model", write_to_comment: false } },
  ] });
  const old = vi.fn(); const listener = vi.fn();
  const ids = await Promise.all([mock.subscribe(old), mock.subscribeCommentaryUpdated(listener)]);
  const started = await mock.startCommentary({ mode: { type: "WholeGame" } });
  expect(started.request_id).toBe(7); expect(started.evidence).toBe("");
  expect(started.issues).toEqual(defaultCommentaryUpdated.issues);
  const delivered = listener.mock.calls[0]?.[0];
  expect(delivered.text).toBe("连接"); expect(delivered.replacement).toBe(false);
  expect(delivered.positions[0].move_number).toBe(0);
  expect(old).not.toHaveBeenCalled();
  expect((await mock.commentaryStatus()).phase).toBe("Verifying");
  expect((await mock.stopCommentary()).phase).toBe("Stopped");
  expect(await mock.commentaryModels()).toEqual(["local-model"]);
  expect(await mock.setCommentaryKey("synthetic-test-key", true)).toEqual({ has_key: true, stored: true, available: true });
  const keyStatus = await mock.commentaryKeyStatus();
  expect(keyStatus.has_key).toBe(false); expect(keyStatus.stored).toBe(false); expect(keyStatus.available).toBe(false);
  expect((await mock.clearCommentaryKey()).has_key).toBe(false);
  const saved = await mock.setCommentarySettings({ model: "local-model" });
  expect(saved.rank).toBe("5k"); expect(saved.write_to_comment).toBe(false);
  expect(mock.recordedCalls()).toHaveLength(8);
  for (const call of [
    () => mock.startCommentary({}), mock.commentaryStatus, mock.stopCommentary, mock.commentaryModels,
    () => mock.setCommentaryKey("test", false), mock.commentaryKeyStatus, mock.clearCommentaryKey,
    () => mock.setCommentarySettings({}),
  ]) await expect(call()).rejects.toThrow("Expected mock command");
  for (const id of ids) await mock.unsubscribe(id);
});

test("older settings acquire commentary defaults and explicit preferences survive", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "settings", result: { version: 1, engine: null, pondering: false } },
    { command: "settings", result: { version: 1, engine: null, pondering: false, commentary: { model: "example-model", remember_key: true, rank: "1d", write_to_comment: false } } },
  ] });
  expect((await mock.settings()).commentary).toEqual(defaultCommentarySettings);
  const explicit = (await mock.settings()).commentary!;
  expect(explicit.model).toBe("example-model"); expect(explicit.remember_key).toBe(true);
  expect(explicit.rank).toBe("1d"); expect(explicit.write_to_comment).toBe(false);
  expect(explicit.detail).toBe("Standard");
});
