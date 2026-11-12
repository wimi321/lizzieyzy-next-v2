import { expect, test, vi } from "vitest";
import { midGame, midGameAnalysis } from "../../e2e/fixtures";
import zh from "../locales/zh-CN.json";
import { boardMenuItems } from "./BoardMenu";

const run = () => ({
  insert: vi.fn(),
  addBranch: vi.fn(),
  previous: vi.fn(),
  find: vi.fn(),
  move: vi.fn(),
  switchStone: vi.fn(),
  deleteStone: vi.fn(),
  review: vi.fn(),
});

test("a stone offers Java's stone menu; an empty point the insert entries; a candidate also its branch", () => {
  const view = midGame();
  const analysis = midGameAnalysis(7, 20);
  const ids = (point: { x: number; y: number }) => boardMenuItems(view, analysis, point, zh.boardMenu, run()).map((item) => item.label);
  expect(ids({ x: 3, y: 3 })).toEqual(["跳到此步", "移动", "反色", "删除", "回看"]);
  expect(ids({ x: 0, y: 0 })).toEqual(["插入黑子", "插入白子", "回退一手"]);
  // K13 is the engine's first choice in the fixture.
  expect(ids({ x: 10, y: 6 })).toEqual(["添加为分支", "插入黑子", "插入白子", "回退一手"]);
  // Analysis of another position offers no branch.
  expect(boardMenuItems(view, midGameAnalysis(7, 19), { x: 10, y: 6 }, zh.boardMenu, run()).map((item) => item.label)[0]).toBe("插入黑子");
});

test("each entry runs its own command", () => {
  const view = midGame();
  const analysis = midGameAnalysis(7, 20);
  const calls = run();
  const [branch, black] = boardMenuItems(view, analysis, { x: 10, y: 6 }, zh.boardMenu, calls);
  branch!.run();
  expect(calls.addBranch).toHaveBeenCalledWith(analysis.candidates[0]);
  black!.run();
  expect(calls.insert).toHaveBeenCalledWith("Black");
  for (const item of boardMenuItems(view, analysis, { x: 3, y: 3 }, zh.boardMenu, calls)) item.run();
  for (const name of ["find", "move", "switchStone", "deleteStone", "review"] as const) expect(calls[name]).toHaveBeenCalledTimes(1);
});
