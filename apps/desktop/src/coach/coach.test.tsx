import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { useEffect } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import type { AssetDownloadStatus, BoardView } from "@/api/backend";
import type { CoachMoveRated, CoachView } from "../api/coach";
import en from "../locales/en.json";
import { isEnabled } from "../menu/bindings";
import { changeFor, headline, hintMarks, moveText, rankAt, rankIndex, rankName } from "./rank";
import { useCoach } from "./useCoach";

afterEach(cleanup);
const labels = en.coach;

test("Java's 29 ranks run from 20 kyu to 9 dan, 3 dan by default", () => {
  expect(rankAt(0)).toEqual({ type: "Kyu", data: 20 });
  expect(rankAt(19)).toEqual({ type: "Kyu", data: 1 });
  expect(rankAt(20)).toEqual({ type: "Dan", data: 1 });
  expect(rankAt(99)).toEqual({ type: "Dan", data: 9 });
  expect(rankIndex(undefined)).toBe(22);
  for (const step of [0, 7, 19, 20, 28]) expect(rankIndex(rankAt(step))).toBe(step);
  expect(rankName({ type: "Dan", data: 3 }, labels)).toBe("3 dan");
});

test("a hint marks the middle of its area first, then the point; moves read as GTP", () => {
  expect(hintMarks({ type: "Region", data: { row: 0, column: 2 } }, 19, 19)).toEqual([{ point: { x: 15, y: 3 }, kind: "Letter", text: "?" }]);
  expect(hintMarks({ type: "Region", data: { row: 1, column: 1 } }, 9, 9)).toEqual([{ point: { x: 4, y: 4 }, kind: "Letter", text: "?" }]);
  const exact = { type: "Exact", data: { mv: { type: "Play", data: { color: "Black", point: { x: 3, y: 3 } } } } } as const;
  expect(hintMarks(exact, 19, 19)).toEqual([{ point: { x: 3, y: 3 }, kind: "Circle", text: null }]);
  expect(hintMarks({ type: "Reading" }, 19, 19)).toEqual([]);
  expect(moveText(exact.data.mv, 19, "pass")).toBe("D16");
  expect(moveText({ type: "Pass", data: { color: "White" } }, 19, "pass")).toBe("pass");
});

test("win chance changes read from the mover's side; the summary says who won", () => {
  expect(changeFor({ color: "White", winrate_change: 0.12 })).toBe(-12);
  expect(changeFor({ color: "Black", winrate_change: -0.234 })).toBe(-23);
  expect(headline({ result: "W+R" }, "White", labels, "Draw")).toBe("You won");
  expect(headline({ result: "W+3.5" }, "Black", labels, "Draw")).toBe("You lost");
  expect(headline({ result: "0" }, "Black", labels, "Draw")).toBe("Draw");
  expect(headline({ result: null }, "Black", labels, "Draw")).toBe("Game over");
});

describe("useCoach", () => {
  type Listener<T> = (value: T) => void;
  const listeners: { coach?: Listener<CoachView>; rated?: Listener<CoachMoveRated>; download?: Listener<AssetDownloadStatus> } = {};
  let backend: ReturnType<typeof makeBackend>;
  function makeBackend() {
    return {
      startCoach: vi.fn(() => Promise.resolve<CoachView>({ active: true, setup: { mode: "LiveFeedback", rank: { type: "Dan", data: 3 } }, human_color: "Black" })),
      coachHint: vi.fn(() => Promise.resolve({ type: "Region", data: { row: 0, column: 0 } } as const)),
      coachDecision: vi.fn(() => Promise.resolve({ type: "Exact", data: { mv: { type: "Play", data: { color: "Black", point: { x: 2, y: 3 } } }, human_share: 0.32 } } as const)),
      coachReview: vi.fn(() => Promise.resolve()),
      humanslModelStatus: vi.fn(() => Promise.resolve({ type: "Missing" } as const)),
      importHumanslModel: vi.fn(),
      pauseAssetDownload: vi.fn(),
      startAssetDownload: vi.fn(() => Promise.resolve<AssetDownloadStatus>({ phase: "Downloading", asset_id: "human-sl", downloaded_bytes: 0, total_bytes: 99_066_230 })),
      cancelAssetDownload: vi.fn(),
      subscribeCoachUpdated: vi.fn((listener: Listener<CoachView>) => ((listeners.coach = listener), Promise.resolve(1))),
      subscribeCoachMoveRated: vi.fn((listener: Listener<CoachMoveRated>) => ((listeners.rated = listener), Promise.resolve(2))),
      subscribeCoachSummary: vi.fn(() => Promise.resolve(3)),
      subscribeGameUpdated: vi.fn(() => Promise.resolve(4)),
      subscribeAssetDownloadProgress: vi.fn((listener: Listener<AssetDownloadStatus>) => ((listeners.download = listener), Promise.resolve(5))),
      unsubscribe: vi.fn(() => Promise.resolve()),
      pickEngineFile: vi.fn(),
      takeBack: vi.fn(),
      resignGame: vi.fn(),
      stopGame: vi.fn(),
    };
  }
  type Hook = ReturnType<typeof useCoach>;
  let latest: Hook | null = null;
  const board = { width: 19, height: 19, stones: [], to_play: "Black", captured_black: 0, captured_white: 0 } as unknown as BoardView;
  function Harness() {
    const hook = useCoach(backend as never, en, { hasStones: false, board, tree: null, pass: vi.fn(), jump: vi.fn() });
    useEffect(() => {
      latest = hook;
    });
    return (
      <>
        {hook.button({ aiCoach: "AI Coach", aiCoachTip: "" })}
        {hook.panel()}
        {hook.dialogs()}
      </>
    );
  }
  beforeEach(() => {
    latest = null;
    backend = makeBackend();
  });

  test("a missing model downloads first, and the game starts by itself once it is in", async () => {
    const view = render(<Harness />);
    await act(async () => undefined);
    expect(isEnabled(latest!.bindings.newHumanSlGame)).toBe(true);
    await act(async () => fireEvent.click(view.getByTestId("coach-button")));
    expect(view.getByTestId("coach-model").dataset.state).toBe("Missing");
    expect(view.getByTestId("coach-start").textContent).toBe("Download and start");
    await act(async () => fireEvent.click(view.getByTestId("coach-start")));
    expect(backend.startAssetDownload).toHaveBeenCalledWith("human-sl");
    expect(view.getByTestId("coach-progress")).toBeTruthy();
    backend.humanslModelStatus.mockResolvedValueOnce({ type: "Ready", data: { path: "/m.bin.gz" } } as never);
    await act(async () => listeners.download?.({ phase: "Completed", asset_id: "human-sl" }));
    expect(backend.startCoach).toHaveBeenCalledWith(expect.objectContaining({ rank: { type: "Dan", data: 3 }, mode: "LiveFeedback", color: "Random", pace: "Natural" }));
    expect(view.queryByTestId("coach-dialog")).toBeNull();
    expect(view.getByTestId("coach-button").textContent).toBe("Coaching");
  });

  test("during 边下边提醒 analysis is hidden, H asks for a hint, and a 恶手 waits for the player's choice", async () => {
    const view = render(<Harness />);
    await act(async () => undefined);
    act(() => listeners.coach?.({ active: true, setup: { mode: "LiveFeedback" }, human_color: "Black" }));
    expect(latest!.hidesAnalysis).toBe(true);
    expect(latest!.shown({ show_winrate_graph: true, show_list_pane: false } as never)).toMatchObject({ show_winrate_graph: false, show_list_pane: true, estimate_location: "Off" });
    await act(async () => latest!.bindings.showHeatmap?.run?.());
    expect(view.getByTestId("coach-hint").dataset.step).toBe("Region");
    expect(latest!.markup(undefined)).toEqual([{ point: { x: 3, y: 3 }, kind: "Letter", text: "?" }]);
    act(() => latest!.bindings.togglePonder?.run?.());
    expect(view.getByTestId("coach-toast").textContent).toBe(labels.hidden);
    act(() => listeners.rated?.({ node_id: 9, move_number: 9, color: "Black", verdict: "Blunder", winrate_change: -0.23, points_lost: 8 }));
    act(() => listeners.coach?.({ active: true, setup: { mode: "LiveFeedback" }, human_color: "Black", awaiting_decision: true }));
    expect(view.getByTestId("coach-blunder").textContent).toContain("Move 9 · Blunder");
    await act(async () => fireEvent.click(view.getByTestId("coach-show-best")));
    expect(backend.coachDecision).toHaveBeenCalledWith("ShowBest");
    expect(view.getByTestId("coach-best").textContent).toContain("C16");
    await act(async () => fireEvent.click(view.getByTestId("coach-retry")));
    expect(backend.coachDecision).toHaveBeenLastCalledWith("Retry");
  });

  test("边下边分析 keeps the analysis; 开始复盘 hands the board back", async () => {
    const view = render(<Harness />);
    await act(async () => undefined);
    act(() => listeners.coach?.({ active: true, setup: { mode: "LiveAnalysis" }, human_color: "White" }));
    expect(latest!.hidesAnalysis).toBe(false);
    expect(latest!.bindings.showHeatmap).toBeUndefined();
    act(() => listeners.coach?.({ active: false, setup: { mode: "LiveAnalysis" }, human_color: "White", summary: { result: "W+R", total_moves: 120, blunders: 1, hints: 0, take_backs: 2, worst_moves: [{ node_id: 40, move_number: 40, verdict: "Blunder", points_lost: 9.5 }] } }));
    expect(view.getByTestId("coach-summary").textContent).toContain("You won");
    await act(async () => fireEvent.click(view.getByTestId("coach-review")));
    expect(backend.coachReview).toHaveBeenCalled();
    expect(view.queryByTestId("coach-panel")).toBeNull();
  });
});
