import { fireEvent, renderHook } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import type { VariationTreeView } from "@/api/backend";
import { branchTarget, mainTrunkTarget } from "./branch";
import { splitHint } from "./hint";
import { matchShortcut, useShortcuts } from "./shortcuts";

describe("shortcut hints, as Java's MenuShortcutHintTest", () => {
  test.each([
    ["保存(Ctrl+S)", "保存", "Ctrl+S"],
    ["Pass (P)", "Pass", "P"],
    ["打开棋谱(O)", "打开棋谱", "O"],
    ["交换黑白(Ctrl+Shift+Alt+→)", "交换黑白", "Ctrl+Shift+Alt+→"],
    ["删除分支(Shift+Delete)", "删除分支", "Shift+Delete"],
    ["终止人机对局(空格)", "终止人机对局", "空格"],
    ["让AI落子最佳一手(逗號)", "让AI落子最佳一手", "逗號"],
    ["跳转到左分支(←)", "跳转到左分支", "←"],
    ["Previous variation ([)", "Previous variation", "["],
    ["清除Lizzie缓存(全局)(Shift+A)", "清除Lizzie缓存(全局)", "Shift+A"],
    ["Set engine rules(KataGo) (Shift+D)", "Set engine rules(KataGo)", "Shift+D"],
    ["终止引擎对局(ALT+R)", "终止引擎对局", "Alt+R"],
    ["Open (Alt + O)", "Open", "Alt+O"],
    ["open (ctrl+o)", "open", "Ctrl+O"],
  ])("%s splits into a label and a shortcut", (text, label, hint) => {
    expect(splitHint(text)).toEqual({ label, hint });
  });

  test.each([
    "浮动主棋盘(保留原棋盘)",
    "整盘精析（推荐）",
    "野狐棋谱(输入野狐昵称获取)",
    "框选分析区域(Alt+鼠标圈选)",
    "Save as (Crtl+Shift+S)",
    "Stop (分析模式 Alt+N)",
    "(O)",
    "新建",
    "",
    "[1] KataGo (B)",
    "2: game (A)",
  ])("%s stays whole", (text) => {
    expect(splitHint(text)).toEqual({ label: text, hint: null });
  });
});

function node(id: number, parent: number | null, children: number[]) {
  return { id, parent, children, move_number: 0, mv: null, move_error: null };
}

// 0 - 1 - 2 - 3          main line
//       |   ` 6
//       ` 4 - 5 - 7      a longer variation
const tree: VariationTreeView = {
  revision: 1,
  root: 0,
  nodes: [node(0, null, [1]), node(1, 0, [2, 4]), node(2, 1, [3, 6]), node(3, 2, []), node(6, 2, []), node(4, 1, [5]), node(5, 4, [7]), node(7, 5, [])],
};

describe("branch navigation, as Java's Board.previousBranch / nextBranch", () => {
  test.each([
    [3, 1, 6],
    [6, 1, 5],
    [5, 1, null],
    [5, -1, 6],
    [2, 1, 4],
    [4, -1, 2],
    [3, -1, null],
    [0, 1, null],
    // Past the end of the main line, its last move leads the list.
    [7, -1, 3],
  ] as const)("from %i in direction %i goes to %s", (from, direction, target) => {
    expect(branchTarget(tree, from, direction)).toBe(target);
  });

  test("back to the main line stops at the deepest ancestor on it", () => {
    expect(mainTrunkTarget(tree, 5)).toBe(1);
    expect(mainTrunkTarget(tree, 7)).toBe(1);
    expect(mainTrunkTarget(tree, 6)).toBe(2);
    expect(mainTrunkTarget(tree, 3)).toBeNull();
    expect(branchTarget(tree, 99, 1)).toBeNull();
  });
});

function press(event: Partial<KeyboardEvent>) {
  return { key: "", code: "", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, repeat: false, ...event };
}

describe("Java's keys", () => {
  test.each([
    [{ code: "KeyO" }, "open"],
    [{ code: "KeyS" }, "saveAs"],
    [{ code: "KeyS", ctrlKey: true }, "save"],
    [{ code: "KeyS", metaKey: true }, "save"],
    [{ code: "KeyS", shiftKey: true }, null],
    [{ code: "KeyO", ctrlKey: true }, null],
    [{ code: "Space" }, "togglePonder"],
    [{ code: "Space", repeat: true }, null],
    [{ code: "Comma" }, "playBestMove"],
    [{ code: "KeyX", altKey: true }, "engineConfig"],
    [{ code: "KeyX" }, null],
    [{ code: "ArrowLeft", repeat: true }, "jumpToLeft"],
    [{ code: "ArrowDown" }, "nextMove"],
    [{ code: "ArrowUp", ctrlKey: true }, "back10"],
    [{ key: "a" }, "autoAnalyze"],
    [{ key: " " }, "togglePonder"],
  ] as const)("%o runs %s", (event, id) => {
    expect(matchShortcut(press(event))).toBe(id);
  });

  test("typing, dialogs and an inactive window keep their keys", () => {
    const run = vi.fn();
    const { rerender, unmount } = renderHook(({ active }) => useShortcuts({ open: { run } }, active), {
      initialProps: { active: true },
    });
    const input = document.createElement("input");
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    const inside = document.createElement("button");
    dialog.append(inside);
    document.body.append(input, dialog);

    fireEvent.keyDown(input, { code: "KeyO" });
    fireEvent.keyDown(inside, { code: "KeyO" });
    expect(run).not.toHaveBeenCalled();
    fireEvent.keyDown(document.body, { code: "KeyO" });
    expect(run).toHaveBeenCalledTimes(1);
    rerender({ active: false });
    fireEvent.keyDown(document.body, { code: "KeyO" });
    expect(run).toHaveBeenCalledTimes(1);

    unmount();
    input.remove();
    dialog.remove();
  });
});
