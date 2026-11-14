import { act, cleanup, render } from "@testing-library/react";
import { useEffect } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { AnalysisExtras, AnalysisView, BoardView, ColorView } from "@/api/backend";
import { drawEstimate } from "../board/estimate";
import type { Geometry } from "../board/geometry";
import { displayBindings } from "../display/bindings";
import { DISPLAY_DEFAULTS, type Display } from "../display/settings";
import { BOTTOM_ITEMS, itemFilter, switchItem, TOP_ITEMS } from "../frame/toolbarItems";
import { isEnabled } from "../menu/bindings";
import { matchShortcut } from "../menu/shortcuts";
import { useEstimate } from "./useEstimate";

afterEach(cleanup);

const OWNERSHIP = [0.9, -0.8, 0.1, -0.3];
const analysis = (extra: Partial<AnalysisView> = {}) =>
  ({ ownership: OWNERSHIP, policy: [0.5, 0.2, 0.1, -1, 0.2], candidates: [], ...extra }) as unknown as AnalysisView;

function estimateBackend() {
  return {
    setAnalysisExtras: vi.fn<(extras: AnalysisExtras) => Promise<void>>(() => Promise.resolve()),
    setPureNetwork: vi.fn<(enabled: boolean) => Promise<void>>(() => Promise.resolve()),
  };
}

type Hook = ReturnType<typeof useEstimate>;
let latest: Hook | null = null;
function expose(hook: Hook) {
  latest = hook;
}

function Harness({ backend, display, set, view }: { backend: ReturnType<typeof estimateBackend>; display: Display; set(patch: Partial<Display>): void; view: AnalysisView | null }) {
  const hook = useEstimate(backend, display, set, view);
  useEffect(() => expose(hook));
  return null;
}

let backend: ReturnType<typeof estimateBackend>;
let set: ReturnType<typeof vi.fn<(patch: Partial<Display>) => void>>;
beforeEach(() => {
  latest = null;
  backend = estimateBackend();
  set = vi.fn<(patch: Partial<Display>) => void>();
});

function mount(display: Partial<Display> = {}, view: AnalysisView | null = analysis()) {
  const result = render(<Harness backend={backend} display={{ ...DISPLAY_DEFAULTS, ...display }} set={set} view={view} />);
  return (next: Partial<Display>) => result.rerender(<Harness backend={backend} display={{ ...DISPLAY_DEFAULTS, ...next }} set={set} view={view} />);
}

test("Kata形势 goes to the boards its location names, each in its style; off by default, as in Java", () => {
  const rerender = mount();
  expect(latest?.on).toBe(false);
  expect(latest?.board.estimate).toBeNull();
  expect(latest?.subBoard.estimate).toBeNull();

  rerender({ estimate_location: "Main", estimate_style: "BySize" });
  expect(latest?.board.estimate).toEqual({ style: "BySize", values: OWNERSHIP });
  expect(latest?.subBoard.estimate).toBeNull();
  rerender({ estimate_location: "Sub" });
  expect(latest?.board.estimate).toBeNull();
  expect(latest?.subBoard.estimate).toEqual({ style: "TransparentSmall", values: OWNERSHIP });
  rerender({ estimate_location: "Both" });
  expect(latest?.board.estimate).not.toBeNull();
  expect(latest?.subBoard.estimate).not.toBeNull();
  // 隐藏: KataGo still answers, nothing is drawn.
  rerender({ estimate_location: "Hidden" });
  expect(latest?.on).toBe(false);
  expect(latest?.board.estimate).toBeNull();
});

test("the bottom bar's Kata形势 and the \".\" key switch it off and back to where it was", () => {
  const rerender = mount({ estimate_location: "Sub" });
  act(() => latest?.toggle());
  expect(set).toHaveBeenLastCalledWith({ estimate_location: "Off" });
  rerender({ estimate_location: "Off" });
  act(() => latest?.toggle());
  expect(set).toHaveBeenLastCalledWith({ estimate_location: "Sub" });
  // The key works only once 使用快捷键 is on.
  expect(isEnabled(latest?.bindings.toggleKataEstimate)).toBe(false);
  rerender({ estimate_location: "Off", estimate_shortcut: true });
  expect(isEnabled(latest?.bindings.toggleKataEstimate)).toBe(true);
});

test("KataGo is asked for the policy and each move's ownership only while something shows them", async () => {
  const rerender = mount();
  await vi.waitFor(() => expect(backend.setAnalysisExtras).toHaveBeenLastCalledWith({ policy: false, moves_ownership: false }));
  // 按选点显示评估 needs each candidate's ownership on the main board.
  rerender({ estimate_location: "Main" });
  await vi.waitFor(() => expect(backend.setAnalysisExtras).toHaveBeenLastCalledWith({ policy: false, moves_ownership: true }));
  rerender({ estimate_location: "Main", moves_ownership: false });
  await vi.waitFor(() => expect(backend.setAnalysisExtras).toHaveBeenLastCalledWith({ policy: false, moves_ownership: false }));

  act(() => latest?.bindings.showPolicy?.run?.());
  await vi.waitFor(() => expect(backend.setAnalysisExtras).toHaveBeenLastCalledWith({ policy: true, moves_ownership: false }));
  expect(latest?.bindings.showPolicy?.checked).toBe(true);
  expect(latest?.board.policy).toEqual([0.5, 0.2, 0.1, -1, 0.2]);
});

test("纯网络 shows the raw policy and the pure estimate style, and goes back if Rust refuses it", async () => {
  mount({ estimate_location: "Both", pure_estimate_style: "BigSquare" });
  act(() => latest?.bindings.showHeatmap?.run?.());
  expect(backend.setPureNetwork).toHaveBeenLastCalledWith(true);
  expect(latest?.pure).toBe(true);
  expect(latest?.board).toMatchObject({ pure: true, policy: [0.5, 0.2, 0.1, -1, 0.2], estimate: { style: "BigSquare" } });
  // The sub-board keeps the ordinary style.
  expect(latest?.subBoard.estimate?.style).toBe("TransparentSmall");
  // The network's own output already has everything: no extras are asked for.
  await vi.waitFor(() => expect(backend.setAnalysisExtras).toHaveBeenLastCalledWith({ policy: false, moves_ownership: false }));

  act(() => latest?.bindings.showHeatmap?.run?.());
  expect(latest?.pure).toBe(false);
  backend.setPureNetwork.mockRejectedValueOnce({ type: "BatchAnalysisRunning" });
  await act(async () => latest?.bindings.showHeatmap?.run?.());
  expect(latest?.pure).toBe(false);
});

test("the sub-board heatmap uses the candidates; 仅棋盘棋子 is passed on", () => {
  const candidates = [{ point: { x: 0, y: 0 }, prior: 0.4, visits: 10 }] as unknown as AnalysisView["candidates"];
  const rerender = mount({ sub_board_heat: "Policy" }, analysis({ candidates }));
  expect(latest?.subBoard.heat).toEqual({ kind: "Policy", candidates });
  rerender({ sub_board_raw: true });
  expect(latest?.subBoard).toMatchObject({ heat: null, raw: true });
});

test("显示 → Kata形势显示, 小棋盘设置 and 工具栏 entries save what they show", () => {
  const set = vi.fn<(patch: Partial<Display>) => void>();
  const bindings = displayBindings({ ...DISPLAY_DEFAULTS, estimate_location: "Main" }, set, vi.fn());
  expect(bindings.kataEstimateOnMainBoard?.checked).toBe(true);
  expect(bindings.kataEstimateByTransparentSmall?.checked).toBe(true);
  expect(bindings.pureEstimateByBigSquare?.checked).toBe(true);
  bindings.kataEstimateOnBothBoard?.run?.();
  bindings.kataEstimateByTransparentNotOnLive?.run?.();
  bindings.pureEstimateBySize?.run?.();
  bindings.kataEstimateSaveState?.run?.();
  bindings.showHeatAfterCalc?.run?.();
  bindings.subBoardShowRaw?.run?.();
  bindings.topToolBarCombined?.run?.();
  expect(set.mock.calls.map(([patch]) => patch)).toEqual([
    { estimate_location: "Both" },
    { estimate_style: "TransparentOffStones" },
    { pure_estimate_style: "BySize" },
    { remember_estimate: true },
    { sub_board_heat: "AfterSearch", sub_board_raw: false },
    { sub_board_heat: "None", sub_board_raw: true },
    { show_top_toolbar: true, top_toolbar_layout: "Combined" },
  ]);
  // 自动换行 only applies to the toolbar's own row.
  expect(isEnabled(bindings.autoWrap)).toBe(true);
  const combined = displayBindings({ ...DISPLAY_DEFAULTS, top_toolbar_layout: "Combined" }, set, vi.fn());
  expect(combined.topToolBarCombined?.checked).toBe(true);
  expect(isEnabled(combined.autoWrap)).toBe(false);
});

test("自定义内容 lists only the items switched away from Java's defaults; PDA and WRN each on its own (Java #709)", () => {
  const set = vi.fn<(patch: Partial<Display>) => void>();
  const bindings = displayBindings(DISPLAY_DEFAULTS, set, vi.fn());
  expect(bindings.showPdaInToolbar?.checked).toBe(true);
  expect(bindings.showTimeLimitController?.checked).toBe(false);
  expect(bindings.openfile?.checked).toBe(false);
  bindings.showPdaInToolbar?.run?.();
  bindings.showTimeLimitController?.run?.();
  bindings.openfile?.run?.();
  expect(set.mock.calls.map(([patch]) => patch)).toEqual([
    { top_toolbar_hidden: ["showPdaInToolbar"] },
    { top_toolbar_hidden: ["showTimeLimitController"] },
    { bottom_toolbar_hidden: ["openfile"] },
  ]);
  const top = itemFilter(TOP_ITEMS, ["showPdaInToolbar", "showTimeLimitController"]);
  expect([top("showPdaInToolbar"), top("showWrnInToolbar"), top("showTimeLimitController")]).toEqual([false, true, true]);
  const bottom = itemFilter(BOTTOM_ITEMS, []);
  expect([bottom("kataEstimateButton"), bottom("openfile"), bottom("unknown")]).toEqual([true, false, false]);
  expect(switchItem(["a", "b"], "a")).toEqual(["b"]);
});

test("Java's H key is 纯网络 and \".\" the Kata形势 shortcut", () => {
  const key = (code: string) => matchShortcut({ key: "", code, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, repeat: false });
  expect(key("KeyH")).toBe("showHeatmap");
  expect(key("Period")).toBe("toggleKataEstimate");
  expect(matchShortcut({ key: ".", code: "", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, repeat: false })).toBe("toggleKataEstimate");
});

/** A 2×2 board in a 100-pixel cell; records the squares drawn. */
function painted(style: Parameters<typeof drawEstimate>[3]["style"], stones: (ColorView | null)[], values: number[]) {
  const squares: { fill: string; size: number }[] = [];
  const context = {
    fillStyle: "",
    fillRect(_x: number, _y: number, width: number) {
      squares.push({ fill: String(this.fillStyle), size: width });
    },
  };
  const geometry = { cell: 100, originX: 50, originY: 50 } as Geometry;
  const board = { width: 2, stones } as unknown as BoardView;
  drawEstimate(context as unknown as CanvasRenderingContext2D, geometry, board, { style, values });
  return squares;
}

test("the five Kata形势 styles follow Java's painters", () => {
  const stones: (ColorView | null)[] = ["Black", "White", null, null];
  // Small squares: black 1/8 of a cell each side, white 1/6; grey on a black stone.
  expect(painted("TransparentSmall", stones, [1, -1, 0.5, 0])).toEqual([
    { fill: "rgb(0, 0, 0)", size: 26 },
    { fill: "rgba(255, 255, 255, 1)", size: 34 },
    { fill: "rgba(0, 0, 0, 0.4980392156862745)", size: 26 },
  ]);
  expect(painted("Transparent", [null, null, null, null], [1, 0, 0, 0])[0]?.size).toBe(50);
  // 不显示在活子上: an owner's own stone gets no square; a dead one does.
  expect(painted("TransparentOffStones", ["Black", "Black", null, null], [0.9, -0.9, 0, 0])).toHaveLength(1);
  // Big squares fill the cell, under the stones.
  expect(painted("BigSquare", stones, [1, -1, 0, 0]).map((square) => square.size)).toEqual([100, 100]);
  // By size: nothing up to 20%, then up to 60% of a cell.
  expect(painted("BySize", stones, [0.2, -1, 0.5, 0]).map((square) => square.size)).toEqual([60, 30]);
});
