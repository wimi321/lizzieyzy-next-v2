import { beforeEach, expect, test, vi } from "vitest";
import type { EngineProfile, Settings } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

const profile: EngineProfile = {
  program: "/fake/katago", model: "/fake/model.bin", config: "/fake/analysis.cfg", extra_args: [],
};

test("engine API preserves arguments, replies and errors", async () => {
  const api = await import("./backend");
  const reply = { accepted: "backend response" };
  const cases: [() => Promise<unknown>, string, object?][] = [
    [() => api.addEngine("CPU", profile), "add_engine", { name: "CPU", profile }],
    [() => api.updateEngine("a", "GPU", profile), "update_engine", { id: "a", name: "GPU", profile }],
    [() => api.removeEngine("a"), "remove_engine", { id: "a" }],
    [() => api.reorderEngines(["b", "a"]), "reorder_engines", { ids: ["b", "a"] }],
    [() => api.selectMainEngine("b"), "select_main_engine", { id: "b" }],
    [() => api.restartEngine(), "restart_engine"],
    [() => api.closeEngine(), "close_engine"],
    [() => api.setEngineSettings({}), "set_engine_settings", { settings: {} }],
    [() => api.previewJavaImport("/Java app"), "preview_java_import", { directory: "/Java app" }],
    [() => api.confirmJavaImport("preview-1"), "confirm_java_import", { previewId: "preview-1" }],
  ];
  for (const [call, command, args] of cases) {
    ipc.invoke.mockResolvedValueOnce(reply);
    expect(await call()).toBe(reply);
    expect(ipc.invoke.mock.calls.at(-1)).toEqual(args === undefined ? [command] : [command, args]);
  }
  const error = { type: "App", data: "engine failed; old engine retained" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.selectMainEngine("broken")).rejects.toBe(error);
});

test("mock engine operations only replay commands, including preview and confirmation", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const settings: Settings = {
    version: 1, engine: profile, pondering: true, fox_history: [],
    move_rank: { mode: "Auto", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12] },
    tsumego: { to_play: "Inherit", attacker: "Auto", ko_threat: "Both", wall_distance: 1 },
  };
  mock.setScript({ commands: [
    { command: "addEngine", result: settings },
    { command: "updateEngine", result: settings },
    { command: "reorderEngines", result: settings },
    { command: "selectMainEngine", result: settings },
    { command: "restartEngine", result: settings },
    { command: "closeEngine", result: settings },
    { command: "removeEngine", result: settings },
    { command: "setEngineSettings", result: {} },
    { command: "previewJavaImport", result: {} },
    { command: "confirmJavaImport", result: settings },
  ] });
  expect(await mock.addEngine("CPU", profile)).toEqual(settings);
  expect(await mock.updateEngine("a", "GPU", profile)).toEqual(settings);
  expect(await mock.reorderEngines(["a"])).toEqual(settings);
  expect(await mock.selectMainEngine("a")).toEqual(settings);
  expect(await mock.restartEngine()).toEqual(settings);
  // Explicit replies are authoritative: the mock does not edit the engine itself.
  expect(await mock.closeEngine()).toEqual(settings);
  expect(await mock.removeEngine("a")).toEqual(settings);
  expect(await mock.setEngineSettings({})).toEqual({});
  expect(await mock.previewJavaImport("/Java app")).toEqual({});
  expect(await mock.confirmJavaImport("preview-1")).toEqual(settings);
  expect(mock.recordedCalls()).toEqual([
    { command: "addEngine", args: ["CPU", profile] },
    { command: "updateEngine", args: ["a", "GPU", profile] },
    { command: "reorderEngines", args: [["a"]] },
    { command: "selectMainEngine", args: ["a"] },
    { command: "restartEngine", args: [] },
    { command: "closeEngine", args: [] },
    { command: "removeEngine", args: ["a"] },
    { command: "setEngineSettings", args: [{}] },
    { command: "previewJavaImport", args: ["/Java app"] },
    { command: "confirmJavaImport", args: ["preview-1"] },
  ]);
  for (const call of [
    () => mock.addEngine("CPU", profile), () => mock.updateEngine("a", "CPU", profile),
    () => mock.removeEngine("a"), () => mock.reorderEngines([]), () => mock.selectMainEngine("a"),
    () => mock.restartEngine(), () => mock.closeEngine(), () => mock.setEngineSettings({}),
    () => mock.previewJavaImport("/Java app"), () => mock.confirmJavaImport("preview-1"),
  ]) await expect(call()).rejects.toThrow("Expected mock command (none)");
});


test("mock defaults expose new settings without changing legacy serialization or explicit values", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const legacy = { version: 1, engine: profile, pondering: true };
  mock.setScript({ commands: [
    { command: "settings", result: legacy },
    { command: "settings", result: {
      ...legacy, engines: [{ id: "saved", name: "Saved", profile }], main_engine_id: "saved",
      engine_settings: { read_komi: false, auto_check_alive: false, default_komi: 0, report_interval_ms: 250 },
    } },
  ] });
  const defaults = await mock.settings();
  expect(defaults.engine).toEqual(profile);
  // As Rust migrates it: the old single engine is a one-item list, and the main one.
  expect(defaults.engines).toEqual([{ id: "legacy-engine", name: "KataGo", profile }]);
  expect(defaults.main_engine_id).toBe("legacy-engine");
  expect(defaults.engine_settings).toEqual({
    rules: { type: "FollowSgf" }, read_komi: true, default_komi: 7.5,
    auto_check_alive: true, report_interval_ms: 100,
  });
  const encoded = JSON.parse(JSON.stringify(defaults)) as Settings;
  expect(encoded.engine).toEqual(profile);
  expect(encoded).not.toHaveProperty("engines");
  expect(encoded).not.toHaveProperty("main_engine_id");
  expect(encoded).not.toHaveProperty("engine_settings");
  const explicit = await mock.settings();
  expect(explicit.main_engine_id).toBe("saved");
  expect(explicit.engines).toEqual([{ id: "saved", name: "Saved", profile }]);
  expect(explicit.engine_settings).toMatchObject({
    read_komi: false, auto_check_alive: false, default_komi: 0, report_interval_ms: 250,
  });
  expect(explicit.engine_settings?.rules).toEqual({ type: "FollowSgf" });
  expect(JSON.parse(JSON.stringify(explicit)).engine_settings).toEqual({
    read_komi: false, auto_check_alive: false, default_komi: 0, report_interval_ms: 250,
  });
});
