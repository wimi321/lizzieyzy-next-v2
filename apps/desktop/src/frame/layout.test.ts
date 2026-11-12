import { expect, test } from "vitest";
import { ALL_PANELS, inFrameLayout, STATUS_HEIGHT } from "./layout";

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

test("hidden panels give their room to their neighbours as Java's do", () => {
  // No comment: the sub-board stays under the graph and the tree takes the whole right column.
  const noComment = inFrameLayout(1429, 833, 19, { ...ALL_PANELS, comment: false });
  expect(noComment.subBoard).toEqual({ x: 0, y: 464, width: 280, height: 280 });
  expect(noComment.tree).toEqual({ x: 1145, y: 0, width: 284, height: 833 });
  // No sub-board: the tree takes the whole right column.
  expect(inFrameLayout(1429, 833, 19, { ...ALL_PANELS, subBoard: false }).tree).toEqual({ x: 1145, y: 0, width: 284, height: 833 });
  // No tree: the sub-board drops to the bottom of the right column.
  expect(inFrameLayout(1429, 833, 19, { ...ALL_PANELS, tree: false }).subBoard).toEqual({ x: 1145, y: 549, width: 284, height: 284 });
  // No status line: the comment reaches the bottom.
  const noStatus = inFrameLayout(1429, 833, 19, { ...ALL_PANELS, status: false });
  expect(noStatus.comment).toEqual({ x: 0, y: 464, width: 280, height: 369 });
  expect(noStatus.status.height).toBe(0);
});

test("the candidate list takes the lower half of the tree's room, or the whole column without the tree", () => {
  const both = inFrameLayout(1429, 833, 19, { ...ALL_PANELS, list: true });
  expect(both.tree).toEqual({ x: 1145, y: 0, width: 284, height: 274 });
  expect(both.list).toEqual({ x: 1145, y: 274, width: 284, height: 274 });
  expect(both.subBoard).toEqual({ x: 1145, y: 548, width: 284, height: 284 });
  const listOnly = inFrameLayout(1429, 833, 19, { ...ALL_PANELS, tree: false, list: true });
  expect(listOnly.list).toEqual({ x: 1145, y: 0, width: 284, height: 548 });
  expect(listOnly.subBoard).toEqual({ x: 1145, y: 548, width: 284, height: 284 });
});
