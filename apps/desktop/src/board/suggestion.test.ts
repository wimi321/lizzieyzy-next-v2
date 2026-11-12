import { expect, test } from "vitest";
import {
  blunderLevel,
  fitFontSize,
  hsb,
  lineSpan,
  lineWidths,
  playoutsText,
  scoreText,
  starRadius,
  suggestionAlphaRatio,
  suggestionColor,
} from "./suggestion";

test("HSB matches java.awt.Color", () => {
  expect(hsb(0, 1, 0.85)).toEqual([217, 0, 0]);
  expect(hsb(1 / 3, 1, 0.85)).toEqual([0, 217, 0]);
  expect(hsb(0.5, 1, 0.85)).toEqual([0, 217, 217]);
});

test("the best move is opaque cyan", () => {
  expect(suggestionColor(1840, 1840, true)).toEqual({ r: 0, g: 217, b: 217, a: 240 });
});

test("other moves go from red to green with their share of visits", () => {
  expect(suggestionColor(1840, 1840, false)).toMatchObject({ r: 0, g: 217, b: 0, a: 240 });
  const quarter = suggestionColor(460, 1840, false);
  // sqrt(1/4) / 3 = 1/6: yellow.
  expect(quarter).toMatchObject({ r: 217, g: 217, b: 0 });
  expect(quarter.a).toBe(Math.trunc(32 + 208 * (Math.log(0.25) / 5 + 1)));
  expect(suggestionColor(1, 1840, false).a).toBe(32);
});

test("opacity bottoms out below 1/150 of the best move's visits", () => {
  expect(suggestionAlphaRatio(1)).toBe(1);
  expect(suggestionAlphaRatio(Math.exp(-5))).toBeCloseTo(0);
  expect(suggestionAlphaRatio(0.001)).toBe(0);
  expect(suggestionAlphaRatio(0)).toBe(0);
});

test("visits are abbreviated as in Java", () => {
  expect(playoutsText(0)).toBe("0");
  expect(playoutsText(999)).toBe("999");
  expect(playoutsText(1000)).toBe("1.0k");
  expect(playoutsText(1840)).toBe("1.8k");
  expect(playoutsText(9949)).toBe("9.9k");
  expect(playoutsText(9950)).toBe("10k");
  expect(playoutsText(123_456)).toBe("123k");
  expect(playoutsText(12_340_000)).toBe("12.3m");
});

test("scores keep one decimal and no plus sign", () => {
  expect(scoreText(1.64)).toBe("1.6");
  expect(scoreText(2)).toBe("2.0");
  expect(scoreText(-0.04)).toBe("0.0");
  expect(scoreText(-3.26)).toBe("-3.3");
});

test("text is as wide as allowed until it hits the height limit", () => {
  // "56.2" 230 px wide at 100 px, in a 41 px cell: width allows 12, height allows 14.35.
  expect(fitFontSize(230, 41 * 0.35, 41 * 0.67)).toBe(12);
  expect(fitFontSize(55, 41 * 0.35, 41 * 0.67)).toBeCloseTo(14.35);
  expect(fitFontSize(0, 10, 10)).toBe(10);
});

test("a played move is graded against the best one", () => {
  expect(blunderLevel(10, 0)).toBe("bad");
  expect(blunderLevel(0, 5)).toBe("bad");
  expect(blunderLevel(1.5, 0)).toBe("doubtful");
  expect(blunderLevel(1.4, 0.9)).toBe("good");
});

test("line widths and star points scale with the board", () => {
  expect(lineWidths(829, 739)).toEqual({ border: 2, inner: 1 });
  expect(lineWidths(1658, 1477).border).toBeCloseTo(1477 / 481);
  expect(lineWidths(1658, 1477).inner).toBeCloseTo(1477 / 750);
  expect(starRadius(829, 829)).toBe(6);
  expect(starRadius(400, 400)).toBe(3);
});

test("grid lines cover whole pixels", () => {
  expect(lineSpan(1)).toEqual([0, 1]);
  expect(lineSpan(2)).toEqual([-1, 2]);
  // A Retina board 1658 px wide: inner lines 1.97 px, border 3.07 px.
  expect(lineSpan(1477 / 750)).toEqual([-1, 2]);
  expect(lineSpan(1477 / 481)).toEqual([-1, 3]);
});
