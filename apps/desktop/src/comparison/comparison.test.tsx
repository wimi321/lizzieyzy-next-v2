import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { AnalysisView, ComparisonView, DocumentView, EngineStatus, LayoutMode, Settings } from "@/api/backend";
import { midGame, midGameAnalysis } from "../../e2e/fixtures";
import { frameLayout, DEFAULT_ARRANGEMENT } from "../frame/arrange";
import { ALL_PANELS } from "../frame/layout";
import { MainPanel } from "../frame/MainPanel";
import zh from "../locales/zh-CN.json";
import type { MenuBindings } from "../menu/bindings";
import { matchShortcut } from "../menu/shortcuts";
import { comparisonMenu, labelOf, type MenuEntry, withoutSearchOnly } from "../menu/spec";
import { useMenus } from "../menu/useMenus";
import { FunctionSearch } from "../research/FunctionSearch";
import { EngineLines } from "./ComparisonBoard";
import { type ComparisonBackend, type ComparisonEngines, comparisonError, useComparison } from "./useComparison";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const labels = zh.comparison;
const SETTINGS = { version: 1, engine: null, pondering: true } as Settings;
const BOARD = midGame().board!;
const DOUBLE: LayoutMode = "DoubleEngine";

describe("双引擎模式's layout", () => {
  test("two equal squares side by side, the status lines under them, nothing else", () => {
    const layout = frameLayout(1600, 860, 19, ALL_PANELS, { ...DEFAULT_ARRANGEMENT, mode: DOUBLE });
    expect(layout.board).toEqual({ x: 0, y: 0, width: 800, height: 800 });
    expect(layout.extra).toEqual({ x: 800, y: 0, width: 800, height: 800 });
    expect(layout.lines).toEqual({ x: 0, y: 800, width: 1600, height: 40 });
    for (const name of ["captured", "statistics", "graph", "comment", "status", "tree", "list", "subBoard"] as const) expect(layout[name].width * layout[name].height).toBe(0);
    // A window too short for two full squares centres the smaller pair.
    const short = frameLayout(1280, 600, 19, ALL_PANELS, { ...DEFAULT_ARRANGEMENT, mode: DOUBLE });
    expect(short.board).toEqual({ x: 80, y: 0, width: 560, height: 560 });
    expect(short.extra.x).toBe(640);
  });

  test("the main panel draws both boards and the lines, and no status line peeking out of an empty region", () => {
    const { container } = render(
      <MainPanel
        boardSize={19}
        arrangement={{ ...DEFAULT_ARRANGEMENT, mode: DOUBLE }}
        regions={{ board: <i />, extra: <i />, lines: <i />, status: <span>kata1</span>, comment: <i /> }}
      />,
    );
    const regions = [...container.querySelectorAll<HTMLElement>(".main-region")].map((region) => region.dataset.region);
    expect(regions).toEqual(expect.arrayContaining(["board", "extra", "lines"]));
    expect(regions).not.toContain("status");
    expect(container.textContent).not.toContain("kata1");
  });

  test("Alt+6 is its shortcut, as in Java", () => {
    expect(matchShortcut({ code: "Digit6", key: "6", altKey: true, ctrlKey: false, metaKey: false, shiftKey: false, repeat: false })).toBe("extraMode2");
  });
});

describe("the second engine menu and 选择对比引擎", () => {
  test("titled with the comparison engine, listing the engines, then restart and close", () => {
    const menu = comparisonMenu(["KataGo", "b18", "b28"], 1);
    expect(labelOf(menu, zh.menu)).toBe("[2]: b18");
    expect(menu.items.map((entry) => (entry.kind === "separator" ? "-" : labelOf(entry, zh.menu)))).toEqual(["[1] KataGo", "[2] b18", "[3] b28", "-", "重启当前引擎", "关闭当前引擎"]);
    expect(labelOf(comparisonMenu(["KataGo"], null), zh.menu)).toBe("未加载引擎");
  });

  test("the strip shows the second menu only in 双引擎模式, and never the search-only entry", () => {
    const search = { text: labels.search, keywords: labels.keywords.split("|") };
    const ids = (shown: boolean) => {
      const { result } = renderHook(() => useMenus(false, [], zh.edit, "KataGo", 3, 30, { names: ["KataGo", "b18"], main: 0 }, null, null, { shown, selected: null, search }));
      return result.current;
    };
    expect(ids(false).map((menu) => menu.id)).not.toContain("engineMenu2");
    const menus = ids(true);
    expect(menus.map((menu) => menu.id).slice(-2)).toEqual(["engineMenu", "engineMenu2"]);
    const engine = menus.find((menu) => menu.id === "engineMenu")!;
    expect(engine.items.some((entry: MenuEntry) => entry.kind !== "separator" && entry.id === "selectComparisonEngine")).toBe(true);
    const strip = withoutSearchOnly(menus).find((menu) => menu.id === "engineMenu")!;
    expect(strip.items.some((entry: MenuEntry) => entry.kind !== "separator" && entry.id === "selectComparisonEngine")).toBe(false);
  });

  test("搜索功能 finds 选择对比引擎 by 第二引擎 and says why it waits for 双引擎模式", () => {
    const search = { text: labels.search, keywords: labels.keywords.split("|") };
    const { result } = renderHook(() => useMenus(false, [], zh.edit, "KataGo", 3, 30, { names: ["KataGo"], main: 0 }, null, null, { shown: false, selected: null, search }));
    const bindings: MenuBindings = { selectComparisonEngine: { run: vi.fn(), enabled: false, reason: labels.unavailable } };
    render(<FunctionSearch menus={result.current} bindings={bindings} menuLabels={zh.menu} labels={zh.research.search} onClose={vi.fn()} />);
    fireEvent.change(screen.getByRole("combobox", { name: zh.research.search.title }), { target: { value: "第二引擎" } });
    const results = screen.getAllByTestId("search-result");
    expect(results[0]?.dataset.entry).toBe("selectComparisonEngine");
    expect(screen.getByTestId("search-detail").textContent).toBe("选择第二引擎前，请先启用对比模式。");
  });
});

function fakeBackend(initial: ComparisonView = { engine_id: null, status: { type: "Stopped" }, analysis: null }) {
  let changed: ((view: ComparisonView) => void) | null = null;
  let updated: ((analysis: AnalysisView) => void) | null = null;
  const backend = {
    comparisonState: vi.fn(() => Promise.resolve(initial)),
    selectComparisonEngine: vi.fn<(id: string | null) => Promise<Settings>>(() => Promise.resolve(SETTINGS)),
    restartComparisonEngine: vi.fn(() => Promise.resolve(SETTINGS)),
    closeComparisonEngine: vi.fn(() => Promise.resolve(SETTINGS)),
    closeOtherEngines: vi.fn(() => Promise.resolve(SETTINGS)),
    closeAllEngines: vi.fn(() => Promise.resolve(SETTINGS)),
    subscribeComparisonChanged: vi.fn((listener: (view: ComparisonView) => void) => {
      changed = listener;
      return Promise.resolve(3);
    }),
    subscribeComparisonAnalysisUpdated: vi.fn((listener: (analysis: AnalysisView) => void) => {
      updated = listener;
      return Promise.resolve(4);
    }),
    unsubscribe: vi.fn(() => Promise.resolve()),
    previewVariation: vi.fn(() => new Promise<never>(() => undefined)),
  } satisfies ComparisonBackend;
  return { backend, change: (view: ComparisonView) => act(() => changed?.(view)), update: (analysis: AnalysisView) => act(() => updated?.(analysis)) };
}

function enginesWith(apply: (settings: Settings) => void = vi.fn()): ComparisonEngines {
  return {
    engines: [{ id: "e1" }, { id: "e2" }, { id: "e3" }],
    main: "e1",
    menuNames: ["KataGo", "b18", "b28"],
    bindings: { engine0: { run: vi.fn(), checked: true }, engine1: { run: vi.fn() }, engine2: { run: vi.fn() } },
    apply,
  };
}

const flush = () => act(async () => undefined);
const READY: EngineStatus = { type: "Ready", data: { version: "1.16.0" } };

async function mount(backend: ComparisonBackend, options: { mode?: LayoutMode; document?: DocumentView | null; enter?: () => void; apply?: () => void } = {}) {
  const host: { current: ReturnType<typeof useComparison> | null } = { current: null };
  const enter = options.enter ?? vi.fn();
  function Harness({ mode, document }: { mode: LayoutMode; document: DocumentView | null }) {
    const value = useComparison(backend, labels, { engines: enginesWith(options.apply), document, mode, enter });
    host.current = value;
    return (
      <div>
        {value.chip}
        {value.board({ board: BOARD, node: 20, revision: 3, coordinates: true, estimate: null, maxCandidates: 10, onPlay: vi.fn() })}
        {value.lines({ name: "KataGo", analysis: midGameAnalysis(3, 20), node: 20, pondering: true })}
      </div>
    );
  }
  const view = render(<Harness mode={options.mode ?? DOUBLE} document={options.document ?? null} />);
  await flush();
  return { ...view, host: () => host.current!, rerender: (mode: LayoutMode, document: DocumentView | null) => view.rerender(<Harness mode={mode} document={document} />) };
}

describe("对比引擎 in the main window", () => {
  test("the menus: the main engine and the comparison engine exclude each other; restart and close need a comparison engine", async () => {
    const { backend, change } = fakeBackend();
    const { host } = await mount(backend);
    expect(host().bindings.restartCurrentEngine2?.enabled).toBe(false);
    expect(host().bindings.shutdownOtherEngine?.enabled).toBe(false);
    expect(host().bindings.comparisonEngine0?.enabled).toBe(false);
    expect(host().menus).toMatchObject({ shown: true, selected: null });

    change({ engine_id: "e2", status: READY, analysis: null });
    const bindings = host().bindings;
    expect(bindings.comparisonEngine1?.checked).toBe(true);
    expect(bindings.engine1?.enabled).toBe(false);
    expect(bindings.engine2).toBeUndefined();
    expect(host().menus.selected).toBe(1);
    act(() => bindings.comparisonEngine2?.run?.());
    act(() => bindings.shutdownOtherEngine?.run?.());
    await flush();
    expect(backend.selectComparisonEngine).toHaveBeenCalledWith("e3");
    expect(backend.closeOtherEngines).toHaveBeenCalledOnce();
  });

  test("closing all engines, restarting and closing the comparison engine apply the settings Rust returns", async () => {
    const apply = vi.fn();
    const { backend } = fakeBackend({ engine_id: "e2", status: READY, analysis: null });
    const { host } = await mount(backend, { apply });
    act(() => host().bindings.restartCurrentEngine2?.run?.());
    await flush();
    act(() => host().bindings.shutdownCurrentEngine2?.run?.());
    await flush();
    act(() => host().bindings.shutdownAllEngine?.run?.());
    await flush();
    expect([backend.restartComparisonEngine, backend.closeComparisonEngine, backend.closeAllEngines].map((fn) => fn.mock.calls.length)).toEqual([1, 1, 1]);
    expect(apply).toHaveBeenCalledTimes(3);
  });

  test("a refused choice says why, in Java's words for the same engine", async () => {
    vi.useFakeTimers();
    const { backend } = fakeBackend();
    backend.selectComparisonEngine.mockRejectedValueOnce({ type: "SameAsMain" });
    const { host } = await mount(backend);
    act(() => host().bindings.comparisonEngine1?.run?.());
    await flush();
    expect(screen.getByTestId("comparison-toast").textContent).toBe("第一引擎不能与第二引擎相同");
    act(() => vi.advanceTimersByTime(4000));
    expect(screen.queryByTestId("comparison-toast")).toBeNull();
    expect(comparisonError({ type: "Engine", data: { reason: "model missing" } }, labels)).toBe("副引擎没有切换成功：model missing");
  });

  test("选择对比引擎 opens the second engine menu in 双引擎模式 and waits otherwise", async () => {
    const { backend } = fakeBackend();
    const { host, rerender } = await mount(backend);
    const strip = document.createElement("div");
    strip.dataset.testid = "menubar";
    const top = document.createElement("button");
    top.dataset.menuId = "engineMenu2";
    const opened = vi.fn();
    top.addEventListener("click", opened);
    strip.append(top);
    document.body.append(strip);
    act(() => host().bindings.selectComparisonEngine?.run?.());
    expect(opened).toHaveBeenCalledOnce();
    strip.remove();
    rerender("Normal", null);
    expect(host().bindings.selectComparisonEngine).toMatchObject({ enabled: false, reason: labels.unavailable });
    expect(host().menus.shown).toBe(false);
  });
});

describe("双引擎模式's board and status lines", () => {
  test("the right board shows the comparison engine's result for this position only", async () => {
    const { backend, update } = fakeBackend({ engine_id: "e2", status: READY, analysis: null });
    await mount(backend);
    expect(screen.getByTestId("comparison-board").dataset.candidates).toBe("0");
    update({ ...midGameAnalysis(3, 19), source: "Live" });
    expect(screen.getByTestId("comparison-board").dataset.candidates).toBe("0");
    update({ ...midGameAnalysis(3, 20), source: "Saved" });
    const board = screen.getByTestId("comparison-board");
    expect(board.dataset.candidates).toBe("6");
    expect(board.dataset.source).toBe("Saved");
    expect(screen.getByTestId("comparison-sub-line").textContent).toContain("黑胜率： 56.2%");
    expect(screen.queryByTestId("comparison-pick")).toBeNull();
  });

  test("the lines: names, analysis on or paused, starting, failure with its reason, none chosen", () => {
    const line = (status: EngineStatus, name: string | null = "b18", pondering = true) => {
      const { unmount } = render(<EngineLines main={{ name: "KataGo", analysis: null }} comparison={{ name, analysis: null, status }} pondering={pondering} labels={labels} />);
      const text = [screen.getByTestId("comparison-main-line").textContent, screen.getByTestId("comparison-sub-line").textContent, screen.getByTestId("comparison-sub-line").dataset.state];
      unmount();
      return text;
    };
    expect(line(READY)).toEqual(["主引擎： KataGo　分析 开启　快捷键：空格计算量： —　黑胜率： —", "副引擎： b18　分析 开启计算量： —　黑胜率： —", "Ready"]);
    expect(line(READY, "b18", false)[1]).toContain("分析 暂停");
    expect(line({ type: "Starting" })[1]).toContain("启动中…");
    expect(line({ type: "Failed", data: { reason: "找不到模型", stderr_tail: [] } })[1]).toContain("没能启动：找不到模型");
    expect(line({ type: "Stopped" }, null)).toEqual(["主引擎： KataGo　分析 开启　快捷键：空格计算量： —　黑胜率： —", "副引擎： 未选择计算量： —　黑胜率： —", "None"]);
  });

  test("no comparison engine chosen: the board says where to choose one; other layouts show neither", async () => {
    const { backend } = fakeBackend();
    const { rerender } = await mount(backend);
    expect(screen.getByTestId("comparison-pick").textContent).toBe("在「副引擎」菜单里选择对比引擎");
    rerender("Normal", null);
    expect(screen.queryByTestId("comparison-board")).toBeNull();
    expect(screen.queryByTestId("comparison-lines")).toBeNull();
  });
});

describe("a game that carries LZ2", () => {
  const game = (path: string, comparison = true): DocumentView => ({ ...midGame(), file_path: path, file_name: path.split("/").pop() ?? null, has_comparison_analysis: comparison });

  test("outside 双引擎模式 the strip offers it; 进入 enters, × hides it until another game", async () => {
    const enter = vi.fn();
    const { backend } = fakeBackend();
    const { rerender } = await mount(backend, { mode: "Normal", document: game("/games/a.sgf"), enter });
    expect(screen.getByTestId("comparison-prompt").textContent).toContain("此棋谱为双引擎分析棋谱，是否进入双引擎模式查看？");
    fireEvent.click(screen.getByTestId("comparison-dismiss"));
    expect(screen.queryByTestId("comparison-prompt")).toBeNull();
    rerender("Normal", game("/games/b.sgf"));
    fireEvent.click(screen.getByTestId("comparison-enter"));
    expect(enter).toHaveBeenCalledOnce();
    rerender("Normal", game("/games/c.sgf", false));
    expect(screen.queryByTestId("comparison-prompt")).toBeNull();
    rerender(DOUBLE, game("/games/d.sgf"));
    expect(screen.queryByTestId("comparison-prompt")).toBeNull();
  });
});
