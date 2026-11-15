import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { DocumentView, ScoringView } from "@/api/backend";
import { midGame, midGameAnalysis } from "../../e2e/fixtures";
import { Board } from "../board/Board";
import { computeGeometry } from "../board/geometry";
import { pointsInRect } from "../board/regions";
import { boardMenuItems } from "../edit/BoardMenu";
import en from "../locales/en.json";
import zh from "../locales/zh-CN.json";
import { matchShortcut } from "../menu/shortcuts";
import { type AnalysisToolbar, ParamFields, RestrictionButtons } from "./AnalysisToolbar";
import { ScoringPanel } from "./ScoringPanel";
import { SearchParamsDialog } from "./SearchParamsDialog";
import { paramOf, paramsOrNull, resultText, scoreNumber, scoreTerms, toolsError } from "./tools";
import { type AnalysisToolsBackend, useAnalysisTools } from "./useAnalysisTools";
import { JAVA_ICONS } from "../frame/icons";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const labels = zh.analysisTools;
type ScoringBreakdown = ScoringView["black"];

function breakdown(change: Partial<ScoringBreakdown> = {}): ScoringBreakdown {
  return { living_stones: 0, territory: 0, dead_stones: 0, captures: 0, komi: 0, handicap_compensation: 0, total: 0, ...change };
}

function scoringView(change: Partial<ScoringView> = {}): ScoringView {
  return {
    node_id: 20,
    rule: "Area",
    dead_stones: [{ x: 3, y: 3 }],
    neutral_points: [],
    ownership: Array<"Black" | "White" | null>(361).fill(null),
    black: breakdown({ living_stones: 120, territory: 62, dead_stones: 1, captures: 3, total: 182 }),
    white: breakdown({ living_stones: 110, territory: 61, dead_stones: 2, captures: 5, komi: 7.5, total: 178.5 }),
    score_lead: 3.5,
    result: "B+3.5",
    ...change,
  };
}

describe("选点限制 on the board", () => {
  const geometry = computeGeometry(19, 19, 760, 760, 1, true);
  const at = (x: number, y: number) => [geometry.originX + x * geometry.cell, geometry.originY + y * geometry.cell] as const;

  test("a rectangle takes the points inside it; it must cross a line both ways", () => {
    const from = at(2.6, 3.4);
    const to = at(4.2, 4.9);
    expect(pointsInRect(geometry, from, to)).toEqual([
      { x: 3, y: 4 },
      { x: 4, y: 4 },
    ]);
    expect(pointsInRect(geometry, to, from)).toHaveLength(2);
    // A click, even right on a point, and a sliver between two lines, take nothing.
    expect(pointsInRect(geometry, at(5, 5), at(5, 5))).toBeNull();
    expect(pointsInRect(geometry, at(5, 5), at(5.1, 5.1))?.length).toBe(1);
    expect(pointsInRect(geometry, at(5.2, 5), at(5.8, 9))).toBeNull();
    expect(pointsInRect(geometry, at(5.2, 5.2), at(5.3, 5.3))).toBeNull();
    // Dragged past the edge, it stops at the board.
    expect(pointsInRect(geometry, at(-3, -3), at(0.5, 0.5))).toEqual([{ x: 0, y: 0 }]);
  });

  test("Alt+click clears and plays nothing; without the region a click plays", () => {
    const onPlay = vi.fn();
    const onClear = vi.fn();
    const onRegion = vi.fn();
    const board = midGame().board!;
    const { rerender } = render(
      <Board board={board} analysis={null} coordinates estimate={null} maxCandidates={0} label="board" onPlay={onPlay} region={{ kind: null, onRegion, onClear }} />,
    );
    const element = screen.getByTestId("board");
    fireEvent.pointerDown(element, { button: 0, altKey: true, clientX: 10, clientY: 10, pointerId: 1 });
    fireEvent.pointerUp(element, { button: 0, altKey: true, clientX: 10, clientY: 10, pointerId: 1 });
    fireEvent.click(element, { altKey: true, clientX: 10, clientY: 10 });
    expect(onClear).toHaveBeenCalledOnce();
    expect(onRegion).not.toHaveBeenCalled();
    expect(onPlay).not.toHaveBeenCalled();
    // A plain press does not start a rectangle unless the toolbar armed one.
    fireEvent.pointerDown(element, { button: 0, clientX: 10, clientY: 10, pointerId: 1 });
    fireEvent.pointerUp(element, { button: 0, clientX: 10, clientY: 10, pointerId: 1 });
    expect(onClear).toHaveBeenCalledOnce();
    rerender(<Board board={board} analysis={null} coordinates estimate={null} maxCandidates={0} label="board" onPlay={onPlay} region={{ kind: "Avoid", onRegion, onClear }} />);
    expect(element.dataset.selecting).toBe("Avoid");
  });
});

describe("the sums of 终局点目", () => {
  test("area counts stones and territory; territory counts territory, prisoners and dead stones", () => {
    const terms = labels.scoring.terms;
    const area = scoringView();
    expect(scoreTerms(area, "black", terms)).toEqual([
      { label: terms.living, value: 120 },
      { label: terms.territory, value: 62 },
    ]);
    expect(scoreTerms(area, "white", terms).map((term) => term.label)).toEqual([terms.living, terms.territory, terms.komi]);
    const territory = scoringView({ rule: "Territory", white: breakdown({ territory: 50, captures: 5, dead_stones: 2, komi: 6.5, handicap_compensation: 0 }) });
    expect(scoreTerms(territory, "black", terms)).toEqual([
      { label: terms.territory, value: 62 },
      { label: terms.captures, value: 3 },
      { label: terms.dead, value: 2 },
    ]);
    const handicap = scoringView({ white: breakdown({ komi: 0.5, handicap_compensation: 2 }) });
    expect(scoreTerms(handicap, "white", terms).slice(-2).map((term) => term.value)).toEqual([0.5, 2]);
  });

  test("the result reads as Java's 黑胜 / 白胜, a draw as one", () => {
    expect(resultText("B+3.5", labels.scoring)).toBe("黑胜 3.5 目");
    expect(resultText("W+12", labels.scoring)).toBe("白胜 12 目");
    expect(resultText("0", labels.scoring)).toBe("和棋");
    expect(scoreNumber(184)).toBe("184");
    expect(scoreNumber(178.5)).toBe("178.5");
  });

  test("the panel lays out both sums, switches the rule, and confirms or cancels", () => {
    const on = { onRule: vi.fn(), onConfirm: vi.fn(), onCancel: vi.fn() };
    render(<ScoringPanel view={scoringView()} labels={labels.scoring} {...on} />);
    expect(screen.getByTestId("scoring-black").textContent).toBe("黑方:活子 120+地 62=182");
    expect(screen.getByTestId("scoring-white").textContent).toBe("白方:活子 110+地 61+贴目 7.5=178.5");
    expect(screen.getByTestId("scoring-result").dataset.winner).toBe("Black");
    fireEvent.click(screen.getByTestId("scoring-rule-area"));
    fireEvent.click(screen.getByTestId("scoring-rule-territory"));
    fireEvent.click(screen.getByTestId("scoring-confirm"));
    fireEvent.click(screen.getByTestId("scoring-cancel"));
    expect(on.onRule).toHaveBeenCalledExactlyOnceWith("Territory");
    expect(on.onConfirm).toHaveBeenCalledOnce();
    expect(on.onCancel).toHaveBeenCalledOnce();
  });
});

describe("引擎参数", () => {
  test("a field is a number in range or empty for the engine's own value", () => {
    expect(paramOf("1.5", "playout_doubling_advantage")).toBe(1.5);
    expect(paramOf(" -3 ", "playout_doubling_advantage")).toBe(-3);
    expect(paramOf("", "wide_root_noise")).toBeNull();
    expect(paramOf("0,2", "wide_root_noise")).toBe(0.2);
    for (const bad of ["3.1", "x", "1e2"]) expect(paramOf(bad, "playout_doubling_advantage")).toBeUndefined();
    expect(paramOf("0", "root_policy_temperature")).toBeUndefined();
    expect(paramsOrNull({ playout_doubling_advantage: null, wide_root_noise: null, root_policy_temperature: null })).toBeNull();
    expect(paramsOrNull({ wide_root_noise: 0 })).toEqual({ wide_root_noise: 0 });
  });

  test("the window saves what is typed, empties as unset; a value out of range keeps OK greyed", () => {
    const onSave = vi.fn();
    render(<SearchParamsDialog params={{ playout_doubling_advantage: 1, wide_root_noise: null, root_policy_temperature: null }} labels={labels.params} onSave={onSave} onClose={vi.fn()} />);
    const pda = screen.getByTestId("params-playout_doubling_advantage");
    expect(pda).toHaveProperty("value", "1");
    fireEvent.change(screen.getByTestId("params-wide_root_noise"), { target: { value: "9" } });
    expect(screen.getByTestId("params-ok")).toHaveProperty("disabled", true);
    fireEvent.change(screen.getByTestId("params-wide_root_noise"), { target: { value: "0.3" } });
    fireEvent.click(screen.getByTestId("params-ok"));
    expect(onSave).toHaveBeenLastCalledWith({ playout_doubling_advantage: 1, wide_root_noise: 0.3, root_policy_temperature: null });
    fireEvent.click(screen.getByTestId("params-reset"));
    expect(pda).toHaveProperty("value", "");
    fireEvent.click(screen.getByTestId("params-ok"));
    expect(onSave).toHaveBeenLastCalledWith(null);
  });

  test("the toolbar's PDA is ticked when set; a typed value applies on Enter, unticking unsets it", () => {
    const onParam = vi.fn();
    const tools = { params: { playout_doubling_advantage: 0.5 }, paramsEnabled: true, onParam } as unknown as AnalysisToolbar;
    render(<ParamFields tools={tools} pda wrn labels={{ pda: "激进度:", wrn: "广度:" }} />);
    expect(screen.getByTestId("tool-pda-on")).toHaveProperty("checked", true);
    expect(screen.getByTestId("tool-wrn-on")).toHaveProperty("checked", false);
    expect(screen.getByTestId("tool-wrn")).toHaveProperty("value", "0.04");
    fireEvent.change(screen.getByTestId("tool-pda"), { target: { value: "1.2" } });
    fireEvent.keyDown(screen.getByTestId("tool-pda"), { key: "Enter" });
    fireEvent.blur(screen.getByTestId("tool-pda"));
    fireEvent.click(screen.getByTestId("tool-pda-on"));
    fireEvent.click(screen.getByTestId("tool-wrn-on"));
    expect(onParam.mock.calls).toEqual([
      ["playout_doubling_advantage", 1.2],
      ["playout_doubling_advantage", null],
      ["wide_root_noise", 0.04],
    ]);
  });
});

function fakeBackend(): AnalysisToolsBackend {
  return {
    setAnalysisRestriction: vi.fn().mockResolvedValue(null),
    addRestrictionPoints: vi.fn().mockResolvedValue(null),
    removeRestrictionPoints: vi.fn().mockResolvedValue(null),
    clearAnalysisRestriction: vi.fn().mockResolvedValue(undefined),
    setRestrictionDepth: vi.fn().mockResolvedValue(undefined),
    setSearchParams: vi.fn().mockImplementation((params) => Promise.resolve(params)),
    continueLadder: vi.fn().mockResolvedValue({ outcome: "Captured", moves: 7 }),
    startScoring: vi.fn().mockResolvedValue(scoringView()),
    toggleScoringPoint: vi.fn().mockResolvedValue(scoringView()),
    setScoringRule: vi.fn().mockResolvedValue(scoringView()),
    commitScoring: vi.fn().mockResolvedValue(9),
    cancelScoring: vi.fn().mockResolvedValue(undefined),
  };
}

function withTools(document: DocumentView, busy = false) {
  const backend = fakeBackend();
  const hook = renderHook(({ doc, busy: now }) => useAnalysisTools(backend, { labels, document: doc, busy: now, openRules: vi.fn(), openWeights: vi.fn() }), {
    initialProps: { doc: document, busy },
  });
  return { backend, hook };
}

describe("the analysis tools' entries", () => {
  test("the right-click entries follow the restriction, as Java's do", () => {
    const view = midGame();
    const ids = (doc: DocumentView, x: number, y: number) => withTools(doc).hook.result.current.menuItems({ x, y }).map((item) => item.id);
    expect(ids(view, 3, 3)).toEqual(["allow-point", "avoid-point"]);
    const allowed = { ...view, analysis_restriction: { kind: "Allow" as const, points: [{ x: 3, y: 3 }, { x: 4, y: 3 }], depth: 5 } };
    expect(ids(allowed, 3, 3)).toEqual(["allow-point", "remove-allow", "avoid-point", "clear-restriction"]);
    expect(ids(allowed, 9, 3)).toEqual(["allow-point", "add-allow", "avoid-point", "clear-restriction"]);
    const avoided = { ...view, analysis_restriction: { kind: "Avoid" as const, points: [{ x: 3, y: 3 }], depth: 5 } };
    expect(ids(avoided, 3, 3)).toEqual(["allow-point", "clear-restriction"]);
    // During a game Rust refuses them, so the menu leaves them out.
    expect(withTools(view, true).hook.result.current.menuItems({ x: 3, y: 3 })).toEqual([]);
    // The board menu adds them after its own entries, on empty points only.
    const extra = () => [{ id: "allow-point", label: "x", run: vi.fn() }];
    const runs = { insert: vi.fn(), addBranch: vi.fn(), previous: vi.fn(), find: vi.fn(), move: vi.fn(), switchStone: vi.fn(), deleteStone: vi.fn(), review: vi.fn() };
    expect(boardMenuItems(view, null, { x: 0, y: 0 }, zh.boardMenu, runs, extra).map((item) => item.id).at(-1)).toBe("allow-point");
    const stone = view.board!.stones.findIndex(Boolean);
    expect(boardMenuItems(view, null, { x: stone % 19, y: Math.floor(stone / 19) }, zh.boardMenu, runs, extra).some((item) => item.id === "allow-point")).toBe(false);
  });

  test("a rectangle adds its points and puts the armed tool down; the menu items call Rust", async () => {
    const { backend, hook } = withTools(midGame());
    act(() => hook.result.current.toolbar.onSelect("Avoid"));
    expect(hook.result.current.board.region?.kind).toBe("Avoid");
    await act(async () => hook.result.current.board.region?.onRegion("Avoid", [{ x: 1, y: 1 }]));
    expect(backend.addRestrictionPoints).toHaveBeenCalledExactlyOnceWith("Avoid", [{ x: 1, y: 1 }]);
    expect(hook.result.current.toolbar.selecting).toBeNull();
    await act(async () => hook.result.current.menuItems({ x: 2, y: 2 })[0]?.run());
    expect(backend.setAnalysisRestriction).toHaveBeenCalledExactlyOnceWith("Allow", [{ x: 2, y: 2 }]);
    act(() => hook.result.current.toolbar.onSelect("Allow"));
    act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })));
    expect(hook.result.current.toolbar.selecting).toBeNull();
  });

  test("while restricted the board shows Rust's restricted analysis of this position only", () => {
    const plain = midGameAnalysis(3, 20);
    const restricted = { ...midGameAnalysis(3, 20), visits: 7 };
    const view = { ...midGame(), analysis_restriction: { kind: "Allow" as const, points: [{ x: 3, y: 3 }], depth: 5 }, restricted_analysis: restricted };
    const { hook } = withTools(view);
    expect(hook.result.current.shown(plain)).toBe(restricted);
    expect(hook.result.current.board.restriction).toEqual({ kind: "Allow", points: [{ x: 3, y: 3 }] });
    hook.rerender({ doc: { ...view, restricted_analysis: { ...restricted, node_id: 5 } }, busy: false });
    expect(hook.result.current.shown(plain)).toBeNull();
    hook.rerender({ doc: midGame(), busy: false });
    expect(hook.result.current.shown(plain)).toBe(plain);
    expect(hook.result.current.chip).toBeNull();
  });

  test("终局点目 is a check entry; while it is on, a click on the board marks a point", async () => {
    const { backend, hook } = withTools(midGame());
    expect(hook.result.current.bindings.scoreGame?.checked).toBe(false);
    await act(async () => hook.result.current.bindings.scoreGame?.run?.());
    expect(backend.startScoring).toHaveBeenCalledOnce();
    hook.rerender({ doc: { ...midGame(), scoring: scoringView() }, busy: false });
    const now = hook.result.current;
    expect(now.bindings.scoreGame?.checked).toBe(true);
    expect(now.board).toMatchObject({ editing: true, region: undefined });
    expect(now.cover?.regions).toEqual(["list", "tree"]);
    await act(async () => (now.board as { onPoint(point: { x: number; y: number }): void }).onPoint({ x: 4, y: 4 }));
    expect(backend.toggleScoringPoint).toHaveBeenCalledExactlyOnceWith({ x: 4, y: 4 });
    await act(async () => now.bindings.scoreGame?.run?.());
    expect(backend.cancelScoring).toHaveBeenCalledOnce();
    expect(now.bindings.continueLadder?.enabled).toBe(false);
  });

  test("自动补完征子 says what it did; a refusal is said the same way", async () => {
    vi.useFakeTimers();
    const { backend, hook } = withTools(midGame());
    // Only the toast is shown and then taken away; the hook stays mounted.
    const toast = () => {
      const shown = render(<>{hook.result.current.windows}</>);
      const text = shown.queryByTestId("tools-toast")?.textContent ?? null;
      shown.unmount();
      return text;
    };
    await act(async () => hook.result.current.bindings.continueLadder?.run?.());
    expect(toast()).toBe("征子成立，已补完 7 手，一次撤销即可还原。");
    vi.mocked(backend.continueLadder).mockRejectedValueOnce({ type: "AnalysisTools", data: { type: "NotLadder" } });
    await act(async () => hook.result.current.bindings.continueLadder?.run?.());
    expect(toast()).toBe(labels.errors.NotLadder);
    act(() => vi.advanceTimersByTime(3200));
    expect(toast()).toBeNull();
  });

  test("depths and parameters are read at start, saved through Rust and kept", async () => {
    const { backend, hook } = withTools(midGame());
    act(() => hook.result.current.load({ search_params: { wide_root_noise: 0.2 }, restriction_depths: { allow: { type: "FirstMove" } } }));
    expect(hook.result.current.toolbar.depths).toEqual({ allow: { type: "FirstMove" }, avoid: { type: "Custom", data: 5 } });
    await act(async () => hook.result.current.toolbar.onDepth("Avoid", { type: "WholeSearch" }));
    expect(backend.setRestrictionDepth).toHaveBeenCalledWith("Avoid", { type: "WholeSearch" });
    expect(hook.result.current.toolbar.depths.avoid).toEqual({ type: "WholeSearch" });
    await act(async () => hook.result.current.toolbar.onParam("playout_doubling_advantage", 1));
    expect(backend.setSearchParams).toHaveBeenLastCalledWith({ wide_root_noise: 0.2, playout_doubling_advantage: 1 });
    await act(async () => hook.result.current.toolbar.onParam("playout_doubling_advantage", null));
    await act(async () => hook.result.current.toolbar.onParam("wide_root_noise", null));
    expect(backend.setSearchParams).toHaveBeenLastCalledWith(null);
    expect(hook.result.current.toolbar.params).toBeNull();
  });

  test("errors read in the interface language", () => {
    expect(toolsError({ type: "Scoring", data: { type: "GameActive" } }, labels.errors)).toBe(labels.errors.GameActive);
    expect(toolsError({ type: "AnalysisTools", data: { type: "InvalidSearchParam", data: "wideRootNoise" } }, labels.errors)).toBe("参数超出范围：wideRootNoise");
    expect(toolsError("boom", labels.errors)).toBe("boom");
  });
});

describe("the toolbar", () => {
  function tools(change: Partial<AnalysisToolbar> = {}): AnalysisToolbar {
    return {
      enabled: true, selecting: null, onSelect: vi.fn(), depths: { allow: { type: "Custom", data: 5 }, avoid: { type: "FirstMove" } }, onDepth: vi.fn(),
      depthLabels: labels.depth, canClear: false, onClear: vi.fn(), params: null, paramsEnabled: true, onParam: vi.fn(), onParams: vi.fn(), onRules: vi.fn(), ...change,
    };
  }

  test("the rectangles arm, the depth menus choose, clear waits for a restriction", () => {
    const now = tools({ selecting: "Allow" });
    render(<RestrictionButtons tools={now} titles={zh.frame.top} icons={JAVA_ICONS} />);
    expect(screen.getByTestId("tool-select-allow").getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByTestId("tool-clear-restriction")).toHaveProperty("disabled", true);
    fireEvent.click(screen.getByTestId("tool-select-avoid"));
    expect(now.onSelect).toHaveBeenCalledWith("Avoid");

    fireEvent.click(screen.getByTestId("tool-allow-depth"));
    const menu = screen.getByTestId("allow-depth-menu");
    expect(menu.querySelector('[aria-checked="true"]')?.textContent).toContain(labels.depth.custom);
    fireEvent.change(screen.getByTestId("depth-custom-input"), { target: { value: "300" } });
    expect(screen.getByTestId("depth-custom")).toHaveProperty("disabled", true);
    fireEvent.change(screen.getByTestId("depth-custom-input"), { target: { value: "12" } });
    fireEvent.keyDown(screen.getByTestId("depth-custom-input"), { key: "Enter" });
    expect(now.onDepth).toHaveBeenLastCalledWith("Allow", { type: "Custom", data: 12 });
    expect(screen.queryByTestId("allow-depth-menu")).toBeNull();
    fireEvent.click(screen.getByTestId("tool-avoid-depth"));
    fireEvent.click(screen.getByTestId("depth-whole"));
    expect(now.onDepth).toHaveBeenLastCalledWith("Avoid", { type: "WholeSearch" });
  });

  test("Alt+D opens 引擎参数, Ctrl+Q 终局点目", () => {
    const key = (code: string, mods: Partial<KeyboardEvent> = {}) =>
      matchShortcut({ key: "", code, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, repeat: false, ...mods });
    expect(key("KeyD", { altKey: true })).toBe("engineParameters");
    expect(key("KeyQ", { ctrlKey: true })).toBe("scoreGame");
    expect(key("KeyQ")).toBe("openUrl");
    expect(en.analysisTools.menu.allow).toBeTruthy();
  });
});
