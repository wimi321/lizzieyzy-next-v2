import { beforeEach, expect, test, vi } from "vitest";
import type { AppEvent, EngineDisplayName, OpenFailed, WireAppEvent } from "./backend";
import { documentView, boardView } from "../../e2e/fixtures";

const ipc = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as { onmessage: (event: WireAppEvent) => void }[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: ipc.invoke,
  Channel: class {
    onmessage: (event: WireAppEvent) => void = () => {};
    constructor() { ipc.channels.push(this); }
  },
}));
beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
  ipc.channels.length = 0;
});
const profile = { program: "katago", model: "weights/default.bin.gz", config: "analysis.cfg", extra_args: [] };
const names: EngineDisplayName[] = [
  { type: "Text", data: "Transformer B11 · 2026-09-25" },
  { type: "Catalog", data: "b10-balanced" },
  { type: "Catalog", data: "b10-lightweight" },
];

test("engineDisplayName forwards profiles and preserves Text/Catalog replies", async () => {
  const backend = await import("./backend");
  for (const name of names) {
    ipc.invoke.mockResolvedValueOnce(name);
    expect(await backend.engineDisplayName(profile)).toEqual(name);
    expect(ipc.invoke).toHaveBeenLastCalledWith("engine_display_name", { profile });
  }
});

test("open failures share the ordered channel without reaching existing UI listeners", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockResolvedValue(42);
  const documents: AppEvent[] = [];
  const failures: OpenFailed[] = [];
  const ordinary = await backend.subscribe((event) => documents.push(event));
  const failed = await backend.subscribeOpenFailed((failure) => failures.push(failure));
  expect(ipc.channels).toHaveLength(1);
  const channel = ipc.channels[0]!;
  const failure = { path: "/games/broken.sgf", reason: "invalid SGF" };
  channel.onmessage({ type: "OpenFailed", data: failure });
  expect(failures).toEqual([failure]);
  expect(documents).toEqual([]);
  await backend.unsubscribe(failed);
  channel.onmessage({ type: "OpenFailed", data: failure });
  expect(failures).toHaveLength(1);
  expect(ipc.invoke).not.toHaveBeenCalledWith("unsubscribe", expect.anything());
  await backend.unsubscribe(ordinary);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 42 });
});

test("mock supports display-name replies and defaults old document filenames to null", async () => {
  const mock = await import("../../e2e/mock-backend");
  const view = documentView(boardView(19, 19, []), 0);
  delete view.file_name;
  const failure = { path: "broken.SGF", reason: "parse failed" };
  mock.setScript({ commands: names.map((name) => ({ command: "engineDisplayName", result: name })) });
  const documents: AppEvent[] = [];
  const failures: OpenFailed[] = [];
  const ordinary = await mock.subscribe((event) => documents.push(event));
  const failed = await mock.subscribeOpenFailed((event) => failures.push(event));
  try {
    mock.pushEvents([
      { type: "DocumentChanged", data: { revision: 1, view } },
      { type: "OpenFailed", data: failure },
      { type: "DocumentChanged", data: { revision: 1, view: { ...view, file_name: "saved.sgf" } } },
    ]);
    expect(documents.map((event) => event.type === "DocumentChanged" ? event.data.view.file_name : undefined)).toEqual([null, "saved.sgf"]);
    expect(failures).toEqual([failure]);
    for (const name of names) expect(await mock.engineDisplayName(profile)).toEqual(name);
    expect(mock.recordedCalls()).toEqual(names.map(() => ({ command: "engineDisplayName", args: [profile] })));
    expect(view.file_name).toBeUndefined();
  } finally {
    await mock.unsubscribe(ordinary);
    await mock.unsubscribe(failed);
  }
});
