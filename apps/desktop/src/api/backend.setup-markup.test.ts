import { beforeEach, expect, test, vi } from "vitest";
import { boardView, documentView } from "../../e2e/fixtures";
import type { AppEvent, Backend, MarkupKindView, MarkupView } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

const point = { x: 2, y: 3 };

function commands(backend: Pick<Backend, "setSetupStone" | "clearSetup" | "setFirstPlayer" | "convertToStartPosition" | "setMarkup" | "clearMarkup" | "toggleToPlay">) {
  return [
    { run: () => backend.setSetupStone(point, "Black"), wire: "set_setup_stone", name: "setSetupStone" as const, args: { point, color: "Black" }, recorded: [point, "Black"] },
    { run: () => backend.setSetupStone(point, "White"), wire: "set_setup_stone", name: "setSetupStone" as const, args: { point, color: "White" }, recorded: [point, "White"] },
    { run: () => backend.setSetupStone(point, null), wire: "set_setup_stone", name: "setSetupStone" as const, args: { point, color: null }, recorded: [point, null] },
    { run: () => backend.clearSetup(), wire: "clear_setup", name: "clearSetup" as const, recorded: [] },
    { run: () => backend.setFirstPlayer("White"), wire: "set_first_player", name: "setFirstPlayer" as const, args: { color: "White" }, recorded: ["White"] },
    { run: () => backend.convertToStartPosition(), wire: "convert_to_start_position", name: "convertToStartPosition" as const, recorded: [] },
    ...(["Triangle", "Square", "Circle", "Cross", "Letter", "Number", "Erase"] as MarkupKindView[]).map((kind) => ({
      run: () => backend.setMarkup(point, kind), wire: "set_markup", name: "setMarkup" as const, args: { point, kind }, recorded: [point, kind],
    })),
    { run: () => backend.clearMarkup(), wire: "clear_markup", name: "clearMarkup" as const, recorded: [] },
    { run: () => backend.toggleToPlay(), wire: "toggle_to_play", name: "toggleToPlay" as const, recorded: [] },
  ];
}

test("setup and markup API forwards every command, kind, revision and rejection", async () => {
  const backend = await import("./backend");
  for (const [index, command] of commands(backend).entries()) {
    ipc.invoke.mockResolvedValueOnce(index + 1);
    expect(await command.run()).toBe(index + 1);
    expect(ipc.invoke.mock.calls.at(-1)).toEqual(command.args ? [command.wire, command.args] : [command.wire]);
    const failure = { type: "App", data: `invalid ${command.wire}` };
    ipc.invoke.mockRejectedValueOnce(failure);
    await expect(command.run()).rejects.toBe(failure);
  }
});

test("mock setup and markup mutations require scripts and replay only supplied state", async () => {
  const mock = await import("../../e2e/mock-backend");
  const scriptedMarkup: MarkupView[] = [{ point: { x: 8, y: 1 }, kind: "Letter", text: "Z" }];
  const view = documentView(boardView(9, 9, []), 0);
  view.markup = scriptedMarkup;
  const changed: AppEvent = { type: "DocumentChanged", data: { revision: 4, view } };
  const requests = commands(mock);
  mock.setScript({ commands: requests.map((command, index) => ({
    command: command.name, result: index + 1,
    events: index === 0 ? [changed] : [],
  })) });
  const listener = vi.fn();
  const id = await mock.subscribe(listener);
  try {
    for (const [index, command] of requests.entries()) expect(await command.run()).toBe(index + 1);
    expect(mock.recordedCalls()).toEqual(requests.map((command) => ({ command: command.name, args: command.recorded })));
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener.mock.calls[0]![0].data.view.markup).toEqual(scriptedMarkup);
    for (const command of requests) await expect(command.run()).rejects.toThrow("Expected mock command (none)");
    expect(listener).toHaveBeenCalledTimes(1);
  } finally {
    await mock.unsubscribe(id);
  }
});

test("mock defaults absent markup without changing legacy scripts or explicit labels", async () => {
  const mock = await import("../../e2e/mock-backend");
  const oldView = documentView(boardView(9, 9, []), 0);
  delete oldView.markup;
  mock.setScript({ initialEvents: [{ type: "DocumentChanged", data: { revision: 1, view: oldView } }] });
  const listener = vi.fn();
  const id = await mock.subscribe(listener);
  try {
    expect(listener.mock.calls[0]![0].data.view.markup).toEqual([]);
    expect(oldView.markup).toBeUndefined();
    const markup: MarkupView[] = [{ point, kind: "Number", text: "42" }];
    mock.pushEvents([{ type: "DocumentChanged", data: { revision: 2, view: { ...oldView, markup } } }]);
    expect(listener.mock.calls.at(-1)![0].data.view.markup).toEqual(markup);
    const error = { type: "App" as const, data: "outside board" };
    for (const command of commands(mock)) {
      mock.setScript({ commands: [{ command: command.name, result: 3, error }] });
      await expect(command.run()).rejects.toEqual(error);
    }
  } finally {
    await mock.unsubscribe(id);
  }
});
