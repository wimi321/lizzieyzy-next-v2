import { expect, test } from "vitest";
import { evaluationText, playoutsText, windowTitle } from "./title";

test("the title reads as LizzieYzy Next's: evaluation, engine and speed, players and result, file", () => {
  expect(
    windowTitle({
      app: "LizzieYzy Next 2",
      evaluation: "[33.6 -0.9 383]",
      engine: "Transformer B11 · 2026-09-25",
      speed: "50 搜索/秒",
      players: "[黑] 样本棋手甲 vs [白] 样本棋手乙",
      result: "(结果:B+R)",
      file: "game.sgf",
    }),
  ).toBe("LizzieYzy Next 2 - [33.6 -0.9 383] Transformer B11 · 2026-09-25 50 搜索/秒 - [黑] 样本棋手甲 vs [白] 样本棋手乙 (结果:B+R) - game.sgf");
});

test("missing parts leave out their separators", () => {
  expect(windowTitle({ app: "LizzieYzy Next 2" })).toBe("LizzieYzy Next 2");
  expect(windowTitle({ app: "LizzieYzy Next 2", speed: "50 搜索/秒", result: "(结果:W+R)" })).toBe(
    "LizzieYzy Next 2 - 50 搜索/秒 - (结果:W+R)",
  );
});

test("visits are shortened as Java's getPlayoutsString does", () => {
  expect(playoutsText(383)).toBe("383");
  expect(playoutsText(1000)).toBe("1.0k");
  expect(playoutsText(1265)).toBe("1.3k");
  expect(playoutsText(13265)).toBe("13k");
  expect(playoutsText(12_345_678)).toBe("12.3m");
});

test("the evaluation gives Black's winrate and lead with one decimal", () => {
  expect(evaluationText(0.336, -0.94, 383)).toBe("[33.6 -0.9 383]");
});
