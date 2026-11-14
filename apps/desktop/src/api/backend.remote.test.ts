import { beforeEach, expect, test, vi } from "vitest";
import type { RemoteEngineInput, Settings } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));
beforeEach(() => { vi.resetModules(); ipc.invoke.mockReset(); });

const websocket: RemoteEngineInput = { type: "WebSocket", data: { url: "ws://127.0.0.1:12345/fixture" } };
const ssh: RemoteEngineInput = { type: "Ssh", data: {
  host: "127.0.0.1", port: 12345, user: "fixture", command: "katago analysis",
  auth: { type: "PrivateKey", data: { path: "fixture-key", passphrase: "fixture-only" } },
} };

test("remote commands forward all inputs and typed failures without interpreting them", async () => {
  const api = await import("./backend");
  const cases: [() => Promise<unknown>, string, object | undefined][] = [
    [() => api.addRemoteEngine("Remote", websocket), "add_remote_engine", { name: "Remote", entry: websocket }],
    [() => api.updateRemoteEngine("engine-1", "SSH", ssh), "update_remote_engine", { id: "engine-1", name: "SSH", entry: ssh }],
    [() => api.testRemoteEngine(ssh), "test_remote_engine", { entry: ssh }],
    [api.switchToLocalEngine, "switch_to_local_engine", undefined],
    [() => api.normalizeRemoteLink("pasted text"), "normalize_remote_link", { text: "pasted text" }],
    [() => api.readRemoteLinkQr("fixture.png"), "read_remote_link_qr", { path: "fixture.png" }],
  ];
  for (const [call, name, args] of cases) {
    await call();
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [name, args] : [name]));
  }
  const error = { type: "RemoteLink", data: "NoQrCode" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.readRemoteLinkQr("fixture.png")).rejects.toBe(error);
});

test("all remote mock entry points require scripts and return scripted results", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const settings: Settings = {
    version: 1, engine: null, pondering: false, recent_files: [], fox_history: [],
    move_rank: { mode: "Auto", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12] },
    tsumego: { to_play: "Inherit", attacker: "Auto", ko_threat: "Both", wall_distance: 1 }, benchmarks: {},
    auto_quick_analyze_on_load: true,
  };
  mock.setScript({ commands: [
    { command: "addRemoteEngine", result: settings },
    { command: "updateRemoteEngine", result: settings },
    { command: "testRemoteEngine", result: "1.16.5-fake" },
    { command: "switchToLocalEngine", result: settings },
    { command: "normalizeRemoteLink", result: "ws://127.0.0.1:12345/scripted" },
    { command: "readRemoteLinkQr", result: "wss://localhost/scripted-image" },
  ] });
  expect(await mock.addRemoteEngine("Remote", websocket)).toEqual(settings);
  expect(await mock.updateRemoteEngine("engine-1", "SSH", ssh)).toEqual(settings);
  expect(await mock.testRemoteEngine(ssh)).toBe("1.16.5-fake");
  expect(await mock.switchToLocalEngine()).toEqual(settings);
  expect(await mock.normalizeRemoteLink("not parsed in TypeScript")).toBe("ws://127.0.0.1:12345/scripted");
  expect(await mock.readRemoteLinkQr("not read in TypeScript")).toBe("wss://localhost/scripted-image");
  expect(mock.recordedCalls()).toHaveLength(6);
  for (const call of [
    () => mock.addRemoteEngine("Remote", websocket), () => mock.updateRemoteEngine("engine-1", "SSH", ssh),
    () => mock.testRemoteEngine(ssh), mock.switchToLocalEngine,
    () => mock.normalizeRemoteLink("text"), () => mock.readRemoteLinkQr("fixture.png"),
  ]) await expect(call()).rejects.toThrow("Expected mock command");
});

test("older local profiles gain optional defaults and explicit remote fields survive", async () => {
  const { fillRemoteProfileDefaults } = await import("../../e2e/fixtures");
  const local = { program: "katago", model: "model", config: "config", extra_args: [] };
  fillRemoteProfileDefaults(local);
  expect("remote" in local).toBe(true);
  expect(JSON.stringify(local)).not.toContain("remote");
  const profile = { ...local, remote: { type: "Ssh" as const, data: {
    host: "127.0.0.1", port: 2222, user: "fixture", command: "katago analysis",
    auth: { type: "PrivateKey" as const, data: { path: "fixture-key" } },
  } } };
  fillRemoteProfileDefaults(profile);
  expect(profile.remote.data.port).toBe(2222);
  expect(profile.remote.data.auth.data.path).toBe("fixture-key");
  expect("version" in profile.remote.data).toBe(true);
});
