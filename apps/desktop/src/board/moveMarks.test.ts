import { describe, expect, it } from "vitest";
import type { MoveRank } from "@/api/backend";
import { moveLabels, type NumberRule, rankedStones } from "./moveMarks";

const MAIN: NumberRule = { window: -1, fromOne: false, branchPoint: null, branchFromOne: true, allInBranch: false };
// Six points: moves 1-5 and a set-up stone (null) and an empty point (null).
const NUMBERS = [1, 2, null, 3, 4, 5];

describe("moveLabels", () => {
  it("writes every move's number for 全部, none for 不显示", () => {
    expect(moveLabels(NUMBERS, MAIN)).toEqual(["1", "2", null, "3", "4", "5"]);
    expect(moveLabels(NUMBERS, { ...MAIN, window: 0 })).toEqual([null, null, null, null, null, null]);
  });

  it("keeps the last N moves, renumbered from 1 when 总是从1开始显示 is on", () => {
    expect(moveLabels(NUMBERS, { ...MAIN, window: 1 })).toEqual([null, null, null, null, null, "5"]);
    expect(moveLabels(NUMBERS, { ...MAIN, window: 3 })).toEqual([null, null, null, "3", "4", "5"]);
    expect(moveLabels(NUMBERS, { ...MAIN, window: 3, fromOne: true })).toEqual([null, null, null, "1", "2", "3"]);
    // A window wider than the game changes nothing.
    expect(moveLabels(NUMBERS, { ...MAIN, window: 10, fromOne: true })).toEqual(["1", "2", null, "3", "4", "5"]);
  });

  it("counts a variation's moves from where it leaves the main line, or carries on", () => {
    const branch = { ...MAIN, branchPoint: 3 };
    expect(moveLabels(NUMBERS, branch)).toEqual(["1", "2", null, "3", "1", "2"]);
    expect(moveLabels(NUMBERS, { ...branch, branchFromOne: false })).toEqual(["1", "2", null, "3", "4", "5"]);
  });

  it("shows every number in a variation when 分支内总是显示所有手数 is on", () => {
    const branch = { ...MAIN, window: 1, branchPoint: 3 };
    expect(moveLabels(NUMBERS, branch)).toEqual([null, null, null, null, null, "2"]);
    expect(moveLabels(NUMBERS, { ...branch, allInBranch: true })).toEqual(["1", "2", null, "3", "1", "2"]);
    // On the main line the option does nothing.
    expect(moveLabels(NUMBERS, { ...MAIN, window: 1, allInBranch: true })).toEqual([null, null, null, null, null, "5"]);
  });
});

describe("rankedStones", () => {
  const nodes = [11, 12, null, 13, 14, 15];
  const ranks = new Map<number, MoveRank>([
    [11, "Blunder"],
    [13, "Good"],
    [14, "Mistake"],
    [15, "Best"],
  ]);

  it("marks ranked stones of the last N moves, never the last move itself", () => {
    expect(rankedStones(NUMBERS, nodes, ranks, -1)).toEqual([
      { index: 0, rank: "Blunder" },
      { index: 3, rank: "Good" },
      { index: 4, rank: "Mistake" },
    ]);
    expect(rankedStones(NUMBERS, nodes, ranks, 3)).toEqual([
      { index: 3, rank: "Good" },
      { index: 4, rank: "Mistake" },
    ]);
    expect(rankedStones(NUMBERS, nodes, ranks, 1)).toEqual([]);
    expect(rankedStones(NUMBERS, nodes, ranks, 0)).toEqual([]);
  });
});
