import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { TsumegoOptions, TsumegoStatus } from "@/api/backend";
import { documentChanged, midGame } from "../../e2e/fixtures";
import { recordedCalls, setScript } from "../../e2e/mock-backend";
import App from "../App";
import zh from "../locales/zh-CN.json";
import { TsumegoChip } from "./TsumegoChip";
import { TsumegoDialog } from "./TsumegoDialog";

beforeEach(() => setScript({}));
afterEach(cleanup);

const SAVED: TsumegoOptions = { to_play: "Black", attacker: "White", ko_threat: "None", wall_distance: 2 };

function status(active: boolean, extra: Partial<TsumegoStatus> = {}): TsumegoStatus {
  return { revision: 3, active, source_node: active ? 3 : null, options: SAVED, attacker: active ? "White" : null, ko_warnings: [], ...extra };
}

test("the window starts from the saved options and confirms what was chosen", async () => {
  const onConfirm = vi.fn(async () => {});
  render(<TsumegoDialog options={SAVED} labels={zh.tsumego} onConfirm={onConfirm} onClose={vi.fn()} />);
  expect(screen.getByTestId("tsumego-to-play-black").getAttribute("aria-checked")).toBe("true");
  expect(screen.getByTestId("tsumego-attacker-white").getAttribute("aria-checked")).toBe("true");
  expect(screen.getByTestId("tsumego-ko-none").getAttribute("aria-checked")).toBe("true");
  expect((screen.getByTestId("tsumego-distance") as HTMLInputElement).value).toBe("2");

  fireEvent.click(screen.getByTestId("tsumego-to-play-inherit"));
  fireEvent.click(screen.getByTestId("tsumego-attacker-auto"));
  fireEvent.click(screen.getByTestId("tsumego-ko-attacker"));
  fireEvent.change(screen.getByTestId("tsumego-distance"), { target: { value: "3" } });
  await act(async () => {
    fireEvent.click(screen.getByTestId("tsumego-confirm"));
  });
  expect(onConfirm).toHaveBeenCalledWith({ to_play: "Inherit", attacker: "Auto", ko_threat: "Attacker", wall_distance: 3 });
});

test("as in Java, a distance that is not a positive number keeps the old one", async () => {
  const onConfirm = vi.fn(async () => {});
  render(<TsumegoDialog options={SAVED} labels={zh.tsumego} onConfirm={onConfirm} onClose={vi.fn()} />);
  fireEvent.change(screen.getByTestId("tsumego-distance"), { target: { value: "0" } });
  await act(async () => {
    fireEvent.click(screen.getByTestId("tsumego-confirm"));
  });
  expect(onConfirm).toHaveBeenCalledWith({ ...SAVED, wall_distance: 2 });
});

test("a problem that cannot be framed says why and stays open", async () => {
  const onConfirm = vi.fn(async () => {
    throw { type: "App", data: "no side of the board leaves room for a wall" };
  });
  render(<TsumegoDialog options={SAVED} labels={zh.tsumego} onConfirm={onConfirm} onClose={vi.fn()} />);
  await act(async () => {
    fireEvent.click(screen.getByTestId("tsumego-confirm"));
  });
  expect(screen.getByTestId("tsumego-error").textContent).toContain("no side of the board leaves room for a wall");
  expect((screen.getByTestId("tsumego-confirm") as HTMLButtonElement).disabled).toBe(false);
});

test("the chip names the attacker, warns when ko threats did not fit and exits", () => {
  const onExit = vi.fn();
  render(<TsumegoChip status={status(true, { ko_warnings: ["Black"] })} labels={zh.tsumego} onExit={onExit} />);
  const chip = screen.getByTestId("tsumego-chip");
  expect(chip.textContent).toContain("白方进攻");
  expect(screen.getByTestId("tsumego-warning").textContent).toBe("劫材放不下");
  expect(chip.getAttribute("title")).toBe("没有布置黑方劫材的空间");
  fireEvent.click(screen.getByTestId("tsumego-exit"));
  expect(onExit).toHaveBeenCalledTimes(1);
});

test("Shift+E opens the window; confirming frames the problem, and the chip leads back", async () => {
  setScript({
    initialEvents: [documentChanged(midGame(), 3)],
    commands: [
      {
        command: "settings",
        result: { version: 1, engine: { program: "/opt/katago", model: "/opt/model.bin.gz", config: "", extra_args: [] }, pondering: true, tsumego: SAVED },
      },
      { command: "generateTsumego", result: 3 },
      { command: "tsumegoStatus", result: status(true) },
      { command: "tsumegoStatus", result: status(true) },
      { command: "exitTsumego", result: 3 },
    ],
  });
  render(<App />);
  await screen.findByTestId("board");
  await act(async () => {
    fireEvent.keyDown(window, { code: "KeyE", key: "E", shiftKey: true });
  });
  expect(screen.getByTestId("tsumego-dialog")).toBeTruthy();
  expect(screen.getByTestId("tsumego-attacker-white").getAttribute("aria-checked")).toBe("true");
  await act(async () => {
    fireEvent.click(screen.getByTestId("tsumego-confirm"));
  });
  expect(screen.queryByTestId("tsumego-dialog")).toBeNull();
  expect((await screen.findByTestId("tsumego-chip")).textContent).toContain("White attacks");
  await act(async () => {
    fireEvent.click(screen.getByTestId("tsumego-exit"));
  });
  expect(screen.queryByTestId("tsumego-chip")).toBeNull();
  expect(recordedCalls().map((call) => call.command)).toEqual(["settings", "generateTsumego", "tsumegoStatus", "tsumegoStatus", "exitTsumego"]);
  expect(recordedCalls()[1]).toEqual({ command: "generateTsumego", args: [SAVED] });
});
