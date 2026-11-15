import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import en from "../locales/en.json";
import zh from "../locales/zh-CN.json";
import type { MenuBindings } from "../menu/bindings";
import { menusFor, type MenuSubmenu } from "../menu/spec";
import { buildCatalog, matchRank, RESULT_LIMIT, searchCatalog, splitShortcut } from "./catalog";
import { FunctionSearch } from "./FunctionSearch";

afterEach(cleanup);

const MENUS = menusFor(false);

describe("the 搜索功能 catalog", () => {
  test("Java's shortcut comes out of the label; other brackets stay", () => {
    expect(splitShortcut("自动播放(Ctrl+A)")).toEqual(["自动播放", "Ctrl+A"]);
    expect(splitShortcut("打开棋谱(O)")).toEqual(["打开棋谱", "O"]);
    expect(splitShortcut("引擎规则(KataGo)(Shift+D)")).toEqual(["引擎规则(KataGo)", "Shift+D"]);
    expect(splitShortcut("开始/停止 分析(空格)")).toEqual(["开始/停止 分析", "空格"]);
    expect(splitShortcut("设置强制计算区域(Alt+鼠标圈选)")).toEqual(["设置强制计算区域(Alt+鼠标圈选)", null]);
    expect(splitShortcut("Save and load")).toEqual(["Save and load", null]);
  });

  test("every menu item is listed with its path and group; submenus open down to their items", () => {
    const catalog = buildCatalog(MENUS, zh.menu, en.menu);
    const autoPlay = catalog.find((entry) => entry.id === "makeAutoPlay");
    expect(autoPlay).toMatchObject({ title: "自动播放", shortcut: "Ctrl+A", path: ["显示"], category: "view" });
    const panel = catalog.find((entry) => entry.id === "ctrlPanel");
    expect(panel).toMatchObject({ path: ["显示", "面板"], category: "view" });
    expect(catalog.find((entry) => entry.id === "saveAndLoad")).toMatchObject({ path: ["文件"], category: "file" });
    // The engine entries of 设置 go with the engines.
    expect(catalog.find((entry) => entry.id === "engineRules")?.category).toBe("engine");
    expect(catalog.find((entry) => entry.id === "languageZH")?.category).toBe("view");
    expect(catalog.some((entry) => entry.id === "panel" || entry.id === "fileMenu")).toBe(false);
    // 最近打开 with nothing in it lists nothing.
    const empty = buildCatalog([{ kind: "submenu", id: "fileMenu", key: null, text: "File", items: [{ kind: "item", id: "recentEmpty", key: null, text: "(none)" }] }], {}, {});
    expect(empty).toEqual([]);
    expect(new Set(catalog.map((entry) => entry.key)).size).toBe(catalog.length);
  });

  test("the interface language and English both find an entry, ranked exact, phrase, path, words", () => {
    const catalog = buildCatalog(MENUS, zh.menu, en.menu);
    const entry = (id: string) => catalog.find((candidate) => candidate.id === id)!;
    expect(matchRank(entry("makeAutoPlay"), "自动播放")).toBe(0);
    expect(matchRank(entry("makeAutoPlay"), "AUTO PLAY")).toBe(0);
    expect(matchRank(entry("makeAutoPlay"), "播放")).toBe(1);
    expect(matchRank(entry("ctrlPanel"), "面板")).toBe(1);
    expect(matchRank(entry("makeAutoPlay"), "显示")).toBe(2);
    expect(matchRank(entry("makeAutoPlay"), "play view")).toBe(3);
    expect(matchRank(entry("makeAutoPlay"), "播放 文件")).toBeNull();
    expect(matchRank(entry("makeAutoPlay"), "  ")).toBeNull();

    expect(searchCatalog(catalog, "存档与读档", () => true)[0]?.id).toBe("saveAndLoad");
    // The entry itself before the toolbar switch of the same name, deeper in the menus.
    expect(searchCatalog(catalog, "自动播放", () => true).map((found) => found.id)).toEqual(["makeAutoPlay", "autoPlay"]);
    // 存档 is exactly the toolbar group's name, so that comes before 存档与读档.
    expect(searchCatalog(catalog, "存档", () => true).map((found) => found.id).slice(0, 2)).toEqual(["showSaveLoadMenu", "saveAndLoad"]);
    expect(searchCatalog(catalog, "a", () => true)).toHaveLength(RESULT_LIMIT);
  });

  test("among equal matches the usable ones come first, then the shallower ones, then the menus' order", () => {
    const menus: MenuSubmenu[] = [{ kind: "submenu", id: "fileMenu", key: null, text: "File", items: [
      { kind: "item", id: "one", key: null, text: "Board one" },
      { kind: "item", id: "two", key: null, text: "Board two" },
      { kind: "item", id: "three", key: null, text: "Board three" },
    ] }];
    const catalog = buildCatalog(menus, {}, {});
    expect(searchCatalog(catalog, "board", (entry) => entry.id !== "one").map((entry) => entry.id)).toEqual(["two", "three", "one"]);
    const nested = buildCatalog([{ ...menus[0]!, items: [{ kind: "submenu", id: "deep", key: null, text: "Deep", items: [{ kind: "item", id: "deeper", key: null, text: "Board" }] }, { kind: "item", id: "top", key: null, text: "Board" }] }], {}, {});
    expect(searchCatalog(nested, "board", () => true).map((entry) => entry.id)).toEqual(["top", "deeper"]);
  });
});

describe("the 搜索功能 window", () => {
  const labels = zh.research.search;

  function open(bindings: MenuBindings) {
    const onClose = vi.fn();
    render(<FunctionSearch menus={MENUS} bindings={bindings} menuLabels={zh.menu} labels={labels} onClose={onClose} />);
    return { onClose, input: screen.getByTestId("search-input") };
  }

  test("an empty field browses Java's groups; a group lists its entries", () => {
    open({});
    expect(screen.getAllByTestId(/^search-category-/).map((button) => button.textContent)).toEqual([labels.all, ...Object.values(labels.categories)]);
    expect(screen.getByTestId("search-category-all").getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByTestId("search-category-sync"));
    const ids = screen.getAllByTestId("search-result").map((row) => row.dataset.entry);
    expect(ids).toContain("foxKifu");
    expect(ids).not.toContain("makeAutoPlay");
    expect(screen.getByTestId("search-count").textContent).toBe(labels.results.replace("{count}", String(ids.length)));
  });

  test("↓ and Enter run the chosen entry after the window closes; the shortcut shows on it", () => {
    const run = vi.fn();
    const calls: string[] = [];
    const { onClose, input } = open({ makeAutoPlay: { run: () => calls.push("run") }, autoAnalyze: { run } });
    onClose.mockImplementation(() => calls.push("close"));
    fireEvent.change(input, { target: { value: "自动" } });
    const rows = screen.getAllByTestId("search-result");
    const at = rows.findIndex((row) => row.dataset.entry === "makeAutoPlay");
    for (let i = 0; i < at; i++) fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(screen.getByRole("option", { selected: true }).dataset.entry).toBe("makeAutoPlay");
    expect(screen.getByTestId("search-detail").textContent).toBe("Ctrl+A");
    fireEvent.keyDown(input, { key: "Enter" });
    expect(calls).toEqual(["close", "run"]);
  });

  test("a word still being composed keeps Enter; Esc closes", () => {
    const run = vi.fn();
    const { onClose, input } = open({ saveAndLoad: { run } });
    fireEvent.change(input, { target: { value: "存档与读档" } });
    fireEvent.keyDown(input, { key: "Enter", isComposing: true });
    fireEvent.keyDown(input, { key: "Enter", keyCode: 229 });
    expect(run).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
    expect(run).not.toHaveBeenCalled();
  });

  test("an entry that cannot run now is greyed and says why; Enter leaves it", () => {
    const { onClose, input } = open({ ctrlPanel: { run: vi.fn(), enabled: false } });
    fireEvent.change(input, { target: { value: "独立棋局控制面板" } });
    const chosen = screen.getByRole("option", { selected: true });
    expect(chosen.getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByTestId("search-detail").textContent).toBe(labels.unavailable);
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: "存档与读档" } });
    expect(screen.getByTestId("search-detail").textContent).toBe(labels.notYet);
  });

  test("nothing found says so", () => {
    const { input } = open({});
    fireEvent.change(input, { target: { value: "zzzz qqqq" } });
    expect(screen.queryAllByTestId("search-result")).toHaveLength(0);
    expect(screen.getByTestId("search-count").textContent).toBe(labels.empty);
  });
});
