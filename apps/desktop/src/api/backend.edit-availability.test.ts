import { beforeEach, expect, test, vi } from "vitest";
import { boardView, documentView } from "../../e2e/fixtures";
import type { WireAppEvent } from "./backend";

beforeEach(() => vi.resetModules());

test("legacy mock document scripts default rule preset and edit availability", async () => {
  const mock = await import("../../e2e/mock-backend");
  const view = documentView(boardView(9, 9, []), 0);
  view.game_info.rules = "Japanese";
  delete view.game_info.rules_preset;
  delete view.can_undo;
  delete view.can_redo;
  const event: WireAppEvent = { type: "DocumentChanged", data: { revision: 4, view } };
  mock.setScript({ initialEvents: [event] });
  const listener = vi.fn();
  const subscription = await mock.subscribe(listener);
  try {
    const received = listener.mock.calls[0]![0].data.view;
    expect(received.game_info.rules).toBe("Japanese");
    expect(received.game_info.rules_preset).toBe("chinese");
    expect(received.can_undo).toBe(false);
    expect(received.can_redo).toBe(false);
    expect(received).toEqual(view);
    expect(view.game_info.rules_preset).toBeUndefined();
    expect(view.can_undo).toBeUndefined();
    expect(view.can_redo).toBeUndefined();
  } finally {
    await mock.unsubscribe(subscription);
  }
});

test("mock edit availability and rule presets come only from scripted events", async () => {
  const mock = await import("../../e2e/mock-backend");
  const view = documentView(boardView(9, 9, []), 0);
  view.game_info.rules = "Original spelling";
  view.game_info.rules_preset = "aga";
  view.can_undo = true;
  view.can_redo = true;
  const event: WireAppEvent = { type: "DocumentChanged", data: { revision: 7, view } };
  mock.setScript({ commands: [{ command: "setComment", result: 7, events: [event] }] });
  const listener = vi.fn();
  const subscription = await mock.subscribe(listener);
  try {
    await mock.setComment("not interpreted by the mock");
    const received = listener.mock.calls[0]![0].data.view;
    expect(received.game_info.rules).toBe("Original spelling");
    expect(received.game_info.rules_preset).toBe("aga");
    expect(received.can_undo).toBe(true);
    expect(received.can_redo).toBe(true);
    expect(received).toEqual(view);
    expect(mock.recordedCalls()).toEqual([{ command: "setComment", args: ["not interpreted by the mock"] }]);
    await expect(mock.undo()).rejects.toThrow("Expected mock command (none)");
    expect(listener).toHaveBeenCalledTimes(1);
  } finally {
    await mock.unsubscribe(subscription);
  }
});
