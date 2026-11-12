import { expect, test } from "vitest";
import type { GraphPointView } from "@/api/backend";
import { jumpTarget } from "./jump";

const point = (node_id: number, move_number: number): GraphPointView => ({ node_id, move_number, summary: null });

test("跳转 goes to the move on the displayed line, else on the main line", () => {
  // A branch leaving the main line after move 2: nodes 0, 1, 2, then 7, 8.
  const branch = [point(0, 0), point(1, 1), point(2, 2), point(7, 3), point(8, 4)];
  const main = [0, 1, 2, 3, 4, 5, 6];
  expect(jumpTarget(branch, main, 4)).toBe(8);
  expect(jumpTarget(branch, main, 1)).toBe(1);
  expect(jumpTarget(branch, main, 6)).toBe(6);
  expect(jumpTarget(branch, main, 9)).toBeNull();
});
