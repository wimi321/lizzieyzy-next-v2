import { beforeEach, expect, test, vi } from "vitest";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

test("clipboard link forwards the nullable Rust reply without arguments", async () => {
  const api = await import("./backend");
  expect(ipc.invoke).not.toHaveBeenCalled();
  for (const result of [null, "https://example.invalid/game.sgf?move=42#position"]) {
    ipc.invoke.mockResolvedValueOnce(result);
    expect(await api.clipboardLink()).toBe(result);
    expect(ipc.invoke).toHaveBeenLastCalledWith("clipboard_link");
  }
  // Clipboard failures become null in AppCore; an IPC failure still propagates.
  const error = { type: "StreamClosed" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.clipboardLink()).rejects.toBe(error);
});

test("mock clipboard queries default to null without consuming unrelated commands", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const link = "https://example.invalid/game.sgf";
  mock.setScript({ commands: [
    { command: "copySgf", result: undefined },
    { command: "clipboardLink", result: link },
    { command: "pasteSgf", result: 7 },
    { command: "clipboardLink", result: null },
  ] });

  expect(await mock.clipboardLink()).toBeNull();
  expect(mock.recordedCalls()).toEqual([]);
  await mock.copySgf();
  expect(await mock.clipboardLink()).toBe(link);
  expect(await mock.clipboardLink()).toBeNull();
  expect(await mock.pasteSgf()).toBe(7);
  expect(await mock.clipboardLink()).toBeNull();
  expect(await mock.clipboardLink()).toBeNull();
  expect(mock.recordedCalls()).toEqual([
    { command: "copySgf", args: [] },
    { command: "clipboardLink", args: [] },
    { command: "pasteSgf", args: [] },
    { command: "clipboardLink", args: [] },
  ]);
});
