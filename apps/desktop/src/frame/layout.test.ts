import { expect, test } from "vitest";
import { inFrameLayout, STATUS_HEIGHT } from "./layout";

test("the status line reserves what Java reserves for one 14 px line", () => {
  expect(STATUS_HEIGHT).toBe(29);
});

test("a 1429 × 969 window (a 1429 × 833 panel) is laid out as LizzieYzy Next lays it out", () => {
  const layout = inFrameLayout(1429, 833);
  expect(layout.board).toEqual({ x: 296, y: 0, width: 833, height: 833 });
  expect(layout.captured).toEqual({ x: 0, y: 0, width: 280, height: 104 });
  expect(layout.statistics).toEqual({ x: 0, y: 104, width: 280, height: 83 });
  expect(layout.graph).toEqual({ x: 0, y: 187, width: 280, height: 277 });
  expect(layout.comment).toEqual({ x: 0, y: 464, width: 280, height: 340 });
  expect(layout.status).toEqual({ x: 0, y: 804, width: 280, height: 29 });
  expect(layout.tree).toEqual({ x: 1145, y: 0, width: 284, height: 548 });
  expect(layout.subBoard).toEqual({ x: 1145, y: 548, width: 284, height: 284 });
});

test("a 1920 × 1080 window (a 1920 × 963 panel, the toolbar on one row) is laid out as LizzieYzy Next lays it out", () => {
  const layout = inFrameLayout(1920, 963);
  expect(layout.board).toEqual({ x: 476, y: 0, width: 963, height: 963 });
  expect(layout.captured).toEqual({ x: 0, y: 0, width: 457, height: 120 });
  expect(layout.statistics).toEqual({ x: 0, y: 120, width: 457, height: 96 });
  expect(layout.graph).toEqual({ x: 0, y: 216, width: 457, height: 321 });
  expect(layout.comment).toEqual({ x: 0, y: 537, width: 457, height: 397 });
  expect(layout.status).toEqual({ x: 0, y: 934, width: 457, height: 29 });
  expect(layout.tree).toEqual({ x: 1458, y: 0, width: 462, height: 500 });
  expect(layout.subBoard).toEqual({ x: 1458, y: 500, width: 462, height: 462 });
});

test("an odd panel height leaves Java's one-pixel gap under the sub-board", () => {
  const layout = inFrameLayout(1429, 835);
  expect(layout.tree.height + layout.subBoard.height).toBe(834);
});
