import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { DisplaySettings, GraphPointView } from "@/api/backend";
import zh from "../locales/zh-CN.json";
import { displayBindings } from "./bindings";
import { DISPLAY_DEFAULTS, filled, hiddenRegions, nextMoveNumbers, nextRankMarks, numberWindow, rankWindow, shiftScores, suggestionStyle } from "./settings";
import { useDisplay } from "./useDisplay";

afterEach(cleanup);

describe("the 显示 entries", () => {
  test("each switch shows the saved setting and saves its opposite", () => {
    const set = vi.fn();
    const bindings = displayBindings({ ...DISPLAY_DEFAULTS, show_winrate_graph: false }, set, vi.fn());
    expect(bindings.coordsMenu?.checked).toBe(true);
    expect(bindings.winrateGraph?.checked).toBe(false);
    expect(bindings.showBlunderBar?.checked).toBe(false);
    bindings.coordsMenu?.run?.();
    bindings.winrateGraph?.run?.();
    bindings.showNewBoardConfirmDialog?.run?.();
    expect(set.mock.calls).toEqual([[{ show_coordinates: false }], [{ show_winrate_graph: true }], [{ confirm_new_board: false }]]);
  });

  test("each choice is checked when it is the saved one; the numbered ones ask first", () => {
    const set = vi.fn();
    const ask = vi.fn();
    const bindings = displayBindings({ ...DISPLAY_DEFAULTS, move_numbers: "LastN", winrate_graph_mode: "ScoreLead" }, set, ask);
    expect(bindings.anyMoveNum?.checked).toBe(true);
    expect(bindings.allMoveNum?.checked).toBe(false);
    expect(bindings.showScoreLeadLineOnly?.checked).toBe(true);
    expect(bindings.rankLastMove?.checked).toBe(true);
    expect(bindings.leadWithKomi?.checked).toBe(true);
    expect(bindings.nextMoveHintInformation?.checked).toBe(true);
    bindings.allMoveNum?.run?.();
    bindings.moveNumberInBranchContinue?.run?.();
    bindings.leadWithoutKomi?.run?.();
    bindings.topToolBarInVisible?.run?.();
    expect(set.mock.calls).toEqual([
      [{ move_numbers: "All" }],
      [{ branch_move_numbers_from_one: false }],
      [{ score_lead_includes_komi: false }],
      [{ show_top_toolbar: false }],
    ]);
    bindings.anyMoveNum?.run?.();
    bindings.setCustomMoves?.run?.();
    bindings.minPlayoutsForNextMove?.run?.();
    expect(ask.mock.calls).toEqual([["numbers"], ["ranks"], ["minVisits"]]);
  });

  test("M and Alt+M step through Java's cycles", () => {
    const at = (change: Partial<typeof DISPLAY_DEFAULTS>) => ({ ...DISPLAY_DEFAULTS, ...change });
    expect(nextMoveNumbers(at({ move_numbers: "None" }))).toEqual({ move_numbers: "All" });
    expect(nextMoveNumbers(at({ move_numbers: "All" }))).toEqual({ move_numbers: "LastOne" });
    expect(nextMoveNumbers(at({ move_numbers: "LastTen" }))).toEqual({ move_numbers: "None" });
    expect(nextRankMarks(at({ move_rank_marks: "All", move_numbers: "All" }))).toEqual({ move_rank_marks: "LastOne", move_numbers: "None" });
    expect(nextRankMarks(at({ move_rank_marks: "LastOne", last_move_rank_marks: 3 }))).toEqual({ move_rank_marks: "LastN", move_numbers: "None" });
    expect(nextRankMarks(at({ move_rank_marks: "LastOne", last_move_rank_marks: 1 }))).toEqual({ move_rank_marks: "None" });
    expect(nextRankMarks(at({ move_rank_marks: "LastN" }))).toEqual({ move_rank_marks: "None" });
    expect(nextRankMarks(at({ move_rank_marks: "None" }))).toEqual({ move_rank_marks: "All" });
  });
});

describe("what the settings mean on screen", () => {
  test("the windows of numbered and marked moves", () => {
    expect(numberWindow(DISPLAY_DEFAULTS)).toBe(0);
    expect(numberWindow({ ...DISPLAY_DEFAULTS, move_numbers: "LastFive" })).toBe(5);
    expect(numberWindow({ ...DISPLAY_DEFAULTS, move_numbers: "LastN", last_move_numbers: 7 })).toBe(7);
    expect(numberWindow({ ...DISPLAY_DEFAULTS, move_numbers: "All" })).toBe(-1);
    expect(rankWindow(DISPLAY_DEFAULTS)).toBe(1);
    expect(rankWindow({ ...DISPLAY_DEFAULTS, move_rank_marks: "LastN" })).toBe(3);
    expect(rankWindow({ ...DISPLAY_DEFAULTS, move_rank_marks: "All" })).toBe(-1);
  });

  test("hidden panels, the candidate style and the score without komi", () => {
    expect(hiddenRegions(DISPLAY_DEFAULTS)).toEqual([]);
    expect(hiddenRegions({ ...DISPLAY_DEFAULTS, show_information_panel: false, show_comment: false, show_status_bar: false })).toEqual([
      "captured",
      "statistics",
      "comment",
      "status",
    ]);
    expect(suggestionStyle({ ...DISPLAY_DEFAULTS, show_visits_in_suggestion: false, winrate_always_black: true }, 6.5)).toMatchObject({
      winrate: true,
      visits: false,
      blackWinrate: true,
      scoreWithKomi: true,
      komi: 6.5,
    });
    const points: GraphPointView[] = [
      { node_id: 0, move_number: 0, summary: null },
      { node_id: 1, move_number: 1, summary: { winrate: 0.5, score_lead: -2, visits: 10 } },
    ];
    expect(shiftScores(points, 0)).toBe(points);
    expect(shiftScores(points, 7.5)[1]?.summary?.score_lead).toBe(5.5);
    expect(shiftScores(points, 7.5)[0]).toBe(points[0]);
  });

  test("fields an older backend leaves out take the defaults", () => {
    const partial: DisplaySettings = { show_coordinates: false };
    expect(filled(partial)).toEqual({ ...DISPLAY_DEFAULTS, show_coordinates: false });
    expect(filled(null)).toEqual(DISPLAY_DEFAULTS);
  });
});

describe("useDisplay", () => {
  const labels = { ...zh.display };

  test("a change shows at once and is saved; a refused one reads the saved settings back", async () => {
    const backend = {
      setDisplaySettings: vi.fn((display: DisplaySettings) => Promise.resolve(display)),
      settings: vi.fn(() => Promise.resolve({ display: { ...DISPLAY_DEFAULTS, show_coordinates: true } } as never)),
    };
    const { result } = renderHook(() => useDisplay(backend, labels));
    act(() => result.current.set({ show_coordinates: false }));
    expect(result.current.display.show_coordinates).toBe(false);
    expect(backend.setDisplaySettings).toHaveBeenCalledWith({ ...DISPLAY_DEFAULTS, show_coordinates: false });

    backend.setDisplaySettings.mockImplementationOnce(() => Promise.reject(new Error("out of range")));
    const errors = vi.spyOn(console, "error").mockImplementation(() => {});
    await act(async () => result.current.set({ show_comment: false }));
    expect(backend.settings).toHaveBeenCalledTimes(1);
    expect(result.current.display.show_comment).toBe(true);
    errors.mockRestore();
  });

  test("最后N手 asks for a number within Java's range, then saves it with the mode", () => {
    const backend = { setDisplaySettings: vi.fn((display: DisplaySettings) => Promise.resolve(display)), settings: vi.fn() };
    function Harness() {
      const display = useDisplay(backend, labels);
      return (
        <>
          <button data-testid="ask" onClick={() => display.bindings.setCustomMoves?.run?.()} />
          {display.dialog}
        </>
      );
    }
    render(<Harness />);
    fireEvent.click(screen.getByTestId("ask"));
    const input = screen.getByTestId("display-ranks-input");
    expect((input as HTMLInputElement).value).toBe("3");
    fireEvent.change(input, { target: { value: "0" } });
    expect(screen.getByTestId("display-ranks-ok")).toHaveProperty("disabled", true);
    fireEvent.change(input, { target: { value: "8" } });
    fireEvent.click(screen.getByTestId("display-ranks-ok"));
    expect(screen.queryByTestId("display-ranks")).toBeNull();
    expect(backend.setDisplaySettings).toHaveBeenCalledWith({ ...DISPLAY_DEFAULTS, move_rank_marks: "LastN", last_move_rank_marks: 8 });
  });
});
