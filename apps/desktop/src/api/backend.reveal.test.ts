import { beforeEach, expect, test, vi } from "vitest";
import type { CommandError } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

test("log directory lookup and reveal are optional APIs that forward the backend path unchanged", async () => {
  const api = await import("./backend");

  const directory = "/app data/日志";
  ipc.invoke.mockResolvedValueOnce(directory);
  expect(await api.logDirectory()).toBe(directory);
  expect(ipc.invoke.mock.calls).toEqual([["log_directory"]]);

  const path = "C:\\app data\\diagnostics\\测试报告.zip";
  ipc.invoke.mockResolvedValueOnce(undefined);
  expect(await api.revealInFolder(path)).toBeUndefined();
  expect(ipc.invoke.mock.calls).toEqual([
    ["log_directory"], ["reveal_in_folder", { path }],
  ]);
});

test("directory and reveal errors pass through IPC without reinterpretation", async () => {
  const api = await import("./backend");
  const lookupError: CommandError = { type: "App", data: "log directory unavailable" };
  ipc.invoke.mockRejectedValueOnce(lookupError);
  await expect(api.logDirectory()).rejects.toBe(lookupError);

  const errors: CommandError[] = [
    { type: "RevealOutsideDataDirectory" },
    { type: "RevealUnavailable", data: "file manager could not be started" },
  ];
  for (const error of errors) {
    ipc.invoke.mockRejectedValueOnce(error);
    await expect(api.revealInFolder("/backend-validates-this-path")).rejects.toBe(error);
    expect(ipc.invoke).toHaveBeenLastCalledWith("reveal_in_folder", { path: "/backend-validates-this-path" });
  }
});

test("mock directory and reveal only replay scripts and preserve path arguments", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const directory = "/scripted/log-directory";
  const path = "/scripted/diagnostics/../report.zip";
  mock.setScript({ commands: [
    { command: "logDirectory", result: directory },
    { command: "revealInFolder", result: undefined },
  ] });
  expect(await mock.logDirectory()).toBe(directory);
  // The mock never normalizes or accepts/rejects a path on Rust's behalf.
  expect(await mock.revealInFolder(path)).toBeUndefined();
  expect(mock.recordedCalls()).toEqual([
    { command: "logDirectory", args: [] },
    { command: "revealInFolder", args: [path] },
  ]);
  // A read-only query: unscripted, it answers a stand-in folder.
  expect(await mock.logDirectory()).toBe("/Users/me/Library/Logs/LizzieYzy Next 2");
  await expect(mock.revealInFolder(path)).rejects.toThrow("Expected mock command (none)");
});

test("mock directory and reveal preserve scripted structured errors", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const lookupError: CommandError = { type: "App", data: "log directory unavailable" };
  const outside: CommandError = { type: "RevealOutsideDataDirectory" };
  const unavailable: CommandError = { type: "RevealUnavailable", data: "no file manager" };
  mock.setScript({ commands: [
    { command: "logDirectory", result: "", error: lookupError },
    { command: "revealInFolder", result: undefined, error: outside },
    { command: "revealInFolder", result: undefined, error: unavailable },
  ] });
  await expect(mock.logDirectory()).rejects.toEqual(lookupError);
  await expect(mock.revealInFolder("/outside")).rejects.toEqual(outside);
  await expect(mock.revealInFolder("/data/report.zip")).rejects.toEqual(unavailable);
  expect(mock.recordedCalls()).toEqual([
    { command: "logDirectory", args: [] },
    { command: "revealInFolder", args: ["/outside"] },
    { command: "revealInFolder", args: ["/data/report.zip"] },
  ]);
});
