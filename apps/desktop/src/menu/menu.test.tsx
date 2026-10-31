import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { VariationTreeView } from "@/api/backend";
import en from "../locales/en.json";
import legacyEn from "../locales/legacy/en-US.json";
import legacyZh from "../locales/legacy/zh-CN.json";
import zhCN from "../locales/zh-CN.json";
import { menuBindings, type MenuActions, type MenuBindings, type MenuState } from "./bindings";
import { MenuBar } from "./MenuBar";
import { engineMenu, labelOf, MENUS, type MenuEntry } from "./spec";

const ALL = [...MENUS, engineMenu("kata1-b18c384nbt")];

function walk(entries: readonly MenuEntry[], visit: (entry: MenuEntry, siblings: readonly MenuEntry[]) => void) {
  for (const entry of entries) {
    visit(entry, entries);
    if (entry.kind === "submenu") walk(entry.items, visit);
  }
}

describe("the menu matches LizzieYzy Next's", () => {
  test("menus in Java's order, the engine's last", () => {
    expect([...MENUS, engineMenu(null)].map((menu) => labelOf(menu, zhCN.menu))).toEqual([
      "文件",
      "显示",
      "棋局",
      "分析",
      "编辑",
      "同步",
      "帮助",
      "设置",
      "跑谱贡献",
      "未加载引擎",
    ]);
    expect(labelOf(ALL[ALL.length - 1]!, zhCN.menu)).toBe("kata1-b18c384nbt");
  });

  test("every label is Java's text in both languages", () => {
    const zh: Record<string, string> = legacyZh;
    const english: Record<string, string> = legacyEn;
    const menuZh: Record<string, string> = zhCN.menu;
    const menuEn: Record<string, string> = en.menu;
    let count = 0;
    walk([...ALL, engineMenu(null)], (entry) => {
      if (entry.kind === "separator" || entry.key === null) return;
      count += 1;
      expect(menuZh[entry.key], entry.key).toBe(zh[entry.key]);
      expect(menuEn[entry.key], entry.key).toBe(english[entry.key]);
    });
    expect(count).toBeGreaterThan(250);
  });

  test("ids are unique and separators only ever divide entries", () => {
    const ids: string[] = [];
    walk(ALL, (entry, siblings) => {
      if (entry.kind !== "separator") {
        ids.push(entry.id);
        return;
      }
      const index = siblings.indexOf(entry);
      expect(index).toBeGreaterThan(0);
      expect(siblings[index + 1]?.kind ?? "separator").not.toBe("separator");
    });
    expect(new Set(ids).size).toBe(ids.length);
  });

  test("entries cut in FEATURES.md and menus Java hides by default are not there", () => {
    const ids = new Set<string>();
    walk(ALL, (entry) => {
      if (entry.kind !== "separator") ids.add(entry.id);
    });
    for (const id of ["forceExit", "appearanceMenu", "theme", "frameLooks", "yikeLiveWeb", "yikeRoom", "quickLinks", "engineMenu2"]) {
      expect(ids.has(id), id).toBe(false);
    }
    for (const id of ["foxKifu", "autoAnalyze", "playerStrengthEstimate", "togglePonder", "jumpToLeft", "about"]) {
      expect(ids.has(id), id).toBe(true);
    }
  });
});

function actions(): MenuActions {
  return {
    open: vi.fn(),
    saveAs: vi.fn(),
    quit: vi.fn(),
    toggleCoordinates: vi.fn(),
    setOwnership: vi.fn(),
    togglePondering: vi.fn(),
    openAutoAnalysis: vi.fn(),
    cancelAutoAnalysis: vi.fn(),
    openStrength: vi.fn(),
    playBestMove: vi.fn(),
    pass: vi.fn(),
    step: vi.fn(),
    goTo: vi.fn(),
    openFox: vi.fn(),
    openEngineSetup: vi.fn(),
    restartEngine: vi.fn(),
    openAbout: vi.fn(),
  };
}

const IDLE: MenuState = {
  hasDocument: true,
  hasBoard: true,
  coordinates: true,
  ownership: false,
  wholeGameActive: false,
  hasBestMove: true,
  engineFailed: false,
  tree: null,
  current: null,
};

function line(...ids: number[]): VariationTreeView {
  return {
    revision: 1,
    root: ids[0] ?? 0,
    nodes: ids.map((id, index) => ({
      id,
      parent: index === 0 ? null : (ids[index - 1] ?? null),
      children: index + 1 < ids.length ? [ids[index + 1] ?? 0] : [],
      move_number: index,
      mv: null,
      move_error: null,
    })),
  };
}

describe("what the menu can do now", () => {
  test("every binding names an entry, or a key Java has without one", () => {
    const ids = new Set(["previousMove", "nextMove", "back10", "forward10"]);
    walk(ALL, (entry) => {
      if (entry.kind !== "separator") ids.add(entry.id);
    });
    for (const id of Object.keys(menuBindings(IDLE, actions()))) expect(ids.has(id), id).toBe(true);
  });

  test("checks follow the board, and entries wait for what they need", () => {
    const run = actions();
    const idle = menuBindings(IDLE, run);
    expect(idle.coordsMenu?.checked).toBe(true);
    expect(idle.kataEstimateClose?.checked).toBe(true);
    expect(idle.kataEstimateOnMainBoard?.checked).toBe(false);
    expect(idle.stopAutoAnalyze?.enabled).toBe(false);
    expect(idle.restartCurrentEngine?.enabled).toBe(false);
    expect(idle.jumpToLeft?.enabled).toBe(false);
    idle.save?.run?.();
    expect(run.saveAs).toHaveBeenCalledTimes(1);
    idle.kataEstimateOnMainBoard?.run?.();
    expect(run.setOwnership).toHaveBeenCalledWith(true);

    const busy = menuBindings({ ...IDLE, wholeGameActive: true, engineFailed: true, hasDocument: false }, run);
    expect(busy.stopAutoAnalyze?.enabled).toBe(true);
    expect(busy.restartCurrentEngine?.enabled).toBe(true);
    expect(busy.saveAs?.enabled).toBe(false);
    expect(busy.jumpToLast?.enabled).toBe(false);
  });

  test("branch entries go where Java would", () => {
    const tree: VariationTreeView = {
      ...line(0, 1, 2),
      nodes: [...line(0, 1, 2).nodes.map((n) => (n.id === 0 ? { ...n, children: [1, 5] } : n)), { id: 5, parent: 0, children: [], move_number: 1, mv: null, move_error: null }],
    };
    const run = actions();
    const at = menuBindings({ ...IDLE, tree, current: 5 }, run);
    expect(at.jumpToRight?.enabled).toBe(false);
    at.jumpToLeft?.run?.();
    at.backToMainBranch?.run?.();
    expect(run.goTo).toHaveBeenNthCalledWith(1, 1);
    expect(run.goTo).toHaveBeenNthCalledWith(2, 0);
  });
});

const user = actions();
const BINDINGS: MenuBindings = menuBindings(IDLE, user);

function renderBar(bindings: MenuBindings = BINDINGS) {
  render(<MenuBar menus={[...MENUS, engineMenu(null)]} bindings={bindings} labels={zhCN.menu} label="菜单栏" />);
  return screen.getByRole("menubar");
}

function row(id: string) {
  const element = document.querySelector<HTMLElement>(`.menu-row[data-menu-id="${id}"]`);
  if (!element) throw new Error(`no row ${id}`);
  return element;
}

const popups = () => screen.queryAllByTestId("menu-popup");

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("the menu strip", () => {
  test("a click opens a menu; Java's shortcut sits right; disabled rows stay put", () => {
    const bar = renderBar();
    fireEvent.click(within(bar).getByText("文件"));
    expect(popups()).toHaveLength(1);
    expect(row("open").querySelector(".menu-label")?.textContent).toBe("打开棋谱");
    expect(row("open").querySelector(".menu-hint")?.textContent).toBe("O");
    expect(row("newBoard").getAttribute("aria-disabled")).toBe("true");
    expect(row("openRecent").getAttribute("aria-disabled")).toBe("true");

    fireEvent.click(row("newBoard"));
    expect(popups()).toHaveLength(1);
    fireEvent.click(row("open"));
    expect(user.open).toHaveBeenCalledTimes(1);
    expect(popups()).toHaveLength(0);
  });

  test("moving along the strip while a menu is open switches menus; a click outside closes", () => {
    const bar = renderBar();
    fireEvent.click(within(bar).getByText("文件"));
    fireEvent.mouseEnter(within(bar).getByText("显示"));
    expect(row("coordsMenu").getAttribute("aria-checked")).toBe("true");
    expect(document.querySelector('.menu-row[data-menu-id="open"]')).toBeNull();
    fireEvent.pointerDown(document.body);
    expect(popups()).toHaveLength(0);
  });

  test("the keyboard walks enabled rows, opens submenus and moves between menus", () => {
    const bar = renderBar();
    const file = within(bar).getByText("文件");
    file.focus();
    fireEvent.keyDown(file, { key: "ArrowDown" });
    expect(row("open").dataset.active).toBe("true");
    const key = (name: string) => fireEvent.keyDown(document.activeElement ?? document.body, { key: name });
    key("ArrowDown");
    expect(row("save").dataset.active).toBe("true");
    key("ArrowUp");
    key("ArrowUp");
    expect(row("exit").dataset.active).toBe("true");

    key("ArrowRight");
    expect(row("panel").dataset.active).toBe("true");
    key("ArrowRight");
    expect(popups()).toHaveLength(2);
    expect(row("winrateGraph").getAttribute("aria-checked")).toBe("true");
    key("ArrowLeft");
    expect(popups()).toHaveLength(1);

    key("Escape");
    expect(popups()).toHaveLength(0);
    expect(document.activeElement).toBe(within(bar).getByText("显示"));
  });

  test("Enter runs the lit entry and closes the menus", () => {
    const bar = renderBar();
    const analyze = within(bar).getByText("分析");
    analyze.focus();
    fireEvent.keyDown(analyze, { key: "Enter" });
    expect(row("togglePonder").dataset.active).toBe("true");
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Enter" });
    expect(user.togglePondering).toHaveBeenCalledTimes(1);
    expect(popups()).toHaveLength(0);
  });

  test("hovering a submenu opens it after Java's delay", () => {
    vi.useFakeTimers();
    const bar = renderBar();
    fireEvent.click(within(bar).getByText("显示"));
    fireEvent.mouseEnter(row("moveRankMenu"));
    expect(popups()).toHaveLength(1);
    act(() => vi.advanceTimersByTime(200));
    expect(popups()).toHaveLength(2);
    expect(row("setCustomMoves").textContent).toContain("显示最后多手:3");
    fireEvent.mouseEnter(row("coordsMenu"));
    act(() => vi.advanceTimersByTime(200));
    expect(popups()).toHaveLength(1);
  });
});
