import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { analysisUpdated, documentChanged, midGame, midGameAnalysis } from "../e2e/fixtures";
import { recordedCalls, setScript } from "../e2e/mock-backend";
import App, { JUMP } from "./App";
import en from "./locales/en.json";
import zhCN from "./locales/zh-CN.json";

beforeEach(() => setScript({}));
afterEach(cleanup);

function keys(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value === null) return [prefix];
  return Object.entries(value).flatMap(([key, child]) => keys(child, prefix ? `${prefix}.${key}` : key));
}

test("zh-CN and en define the same keys", () => {
  expect(keys(zhCN).sort()).toEqual(keys(en).sort());
});

test("renders the board, move number and analysis from backend events", async () => {
  setScript({ initialEvents: [documentChanged(midGame(), 3), analysisUpdated(midGameAnalysis(3, 20))] });
  render(<App />);
  expect(await screen.findByTestId("board")).toBeTruthy();
  expect(screen.getByTestId("move-number").textContent).toBe("20");
  expect(screen.getByTestId("analysis-move").dataset.revision).toBe("3");
});

test("the analysis chip shows the search speed while the engine reports one", async () => {
  setScript({
    initialEvents: [documentChanged(midGame(), 3), analysisUpdated({ ...midGameAnalysis(3, 20), visits_per_second: 1234.4 })],
  });
  render(<App />);
  const speed = await screen.findByTestId("analysis-speed");
  expect(speed.textContent).toBe("1.2k/s");
  expect(screen.getByTestId("analysis-move").getAttribute("title")).toContain("1234 visits per second");
});

test("no speed is shown until the engine has measured one", async () => {
  setScript({ initialEvents: [documentChanged(midGame(), 3), analysisUpdated({ ...midGameAnalysis(3, 20), visits_per_second: null })] });
  render(<App />);
  await screen.findByTestId("analysis-move");
  expect(screen.queryByTestId("analysis-speed")).toBeNull();
});

test("shows why a recorded position cannot be displayed", async () => {
  const view = { ...midGame(), board: null, position_error: "illegal move at node 7" };
  setScript({ initialEvents: [documentChanged(view, 2)] });
  render(<App />);
  expect((await screen.findByTestId("position-error")).textContent).toContain("illegal move at node 7");
});

test("navigation keys call step as in Java (up and down are moves); keys typed into controls do not", async () => {
  setScript({
    initialEvents: [documentChanged(midGame(), 3)],
    commands: [
      { command: "settings", result: { version: 1, engine: { program: "/opt/katago", model: "/opt/model.bin.gz", config: "/opt/analysis.cfg", extra_args: [] }, pondering: true } },
      { command: "step", result: 4 },
      { command: "step", result: 5 },
    ],
  });
  render(<App />);
  await screen.findByTestId("board");
  await act(async () => {
    fireEvent.keyDown(window, { key: "End" });
    fireEvent.keyDown(screen.getByTestId("problems-sort"), { key: "ArrowUp" });
    fireEvent.keyDown(window, { key: "ArrowUp" });
  });
  expect(recordedCalls()).toEqual([
    { command: "settings", args: [] },
    { command: "step", args: [JUMP] },
    { command: "step", args: [-1] },
  ]);
});

test("the pause button starts from the saved setting", async () => {
  setScript({
    initialEvents: [documentChanged(midGame(), 3)],
    commands: [{ command: "settings", result: { version: 1, engine: { program: "/opt/katago", model: "/opt/model.bin.gz", config: "/opt/analysis.cfg", extra_args: [] }, pondering: false } }],
  });
  render(<App />);
  const button = await screen.findByTestId("toggle-pondering");
  await act(async () => {});
  expect(button.getAttribute("aria-pressed")).toBe("true");
});
