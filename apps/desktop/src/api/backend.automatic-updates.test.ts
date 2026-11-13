import { beforeEach, expect, test, vi } from "vitest";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

test("automatic update check forwards null or release data without arguments", async () => {
  const api = await import("./backend");
  expect(ipc.invoke).not.toHaveBeenCalled();
  ipc.invoke.mockResolvedValueOnce(null);
  expect(await api.automaticUpdateCheck()).toBeNull();
  expect(ipc.invoke).toHaveBeenLastCalledWith("automatic_update_check");
  const release = { available: true, version: "0.2.0", notes: "Release notes", published_at: null };
  ipc.invoke.mockResolvedValueOnce(release);
  expect(await api.automaticUpdateCheck()).toBe(release);
  expect(ipc.invoke).toHaveBeenLastCalledWith("automatic_update_check");
  // Network failures become null in Rust; an IPC transport failure still
  // propagates unchanged through this thin wrapper.
  const error = { type: "StreamClosed" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.automaticUpdateCheck()).rejects.toBe(error);
});

test("importing the API and subscribing do not trigger automatic update checks", async () => {
  const api = await import("./backend");
  expect(ipc.invoke).not.toHaveBeenCalled();
  ipc.invoke.mockResolvedValueOnce(71);
  const id = await api.subscribe(vi.fn());
  expect(ipc.invoke.mock.calls.map(([name]) => name)).toEqual(["subscribe"]);
  await api.unsubscribe(id);
  expect(ipc.invoke.mock.calls.map(([name]) => name)).toEqual(["subscribe", "unsubscribe"]);
});

test("mock automatic checks default to null without consuming unrelated scripts and allow scripted overrides", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const release = { available: true, version: "0.2.0" };
  mock.setScript({ commands: [
    { command: "setLanguage", result: "En" },
    { command: "automaticUpdateCheck", result: release },
    { command: "downloadUpdate", result: { phase: "Downloading" } },
    { command: "automaticUpdateCheck", result: null },
    { command: "logDirectory", result: "/data/logs" },
  ] });
  expect(await mock.automaticUpdateCheck()).toBeNull();
  expect(mock.recordedCalls()).toEqual([]);
  expect(await mock.setLanguage("En")).toBe("En");
  const checked = await mock.automaticUpdateCheck();
  expect(checked).toEqual(release);
  expect(checked?.notes).toBeNull();
  expect(checked?.published_at).toBeNull();
  expect(await mock.automaticUpdateCheck()).toBeNull();
  expect((await mock.downloadUpdate()).phase).toBe("Downloading");
  expect(await mock.automaticUpdateCheck()).toBeNull();
  expect(await mock.logDirectory()).toBe("/data/logs");
  expect(await mock.automaticUpdateCheck()).toBeNull();
  expect(mock.recordedCalls()).toEqual([
    { command: "setLanguage", args: ["En"] },
    { command: "automaticUpdateCheck", args: [] },
    { command: "downloadUpdate", args: [] },
    { command: "automaticUpdateCheck", args: [] },
    { command: "logDirectory", args: [] },
  ]);
});
