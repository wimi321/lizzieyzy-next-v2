import { expect, test } from "vitest";
import type { VariationNodeView, VariationTreeView } from "@/api/backend";
import { layoutTree } from "./treeLayout";

/** A tree from parent links; children keep the order given. */
function tree(parents: Array<[id: number, parent: number | null]>): VariationTreeView {
  const nodes: VariationNodeView[] = parents.map(([id, parent]) => ({
    id,
    parent,
    children: parents.filter(([, p]) => p === id).map(([child]) => child),
    move_number: 0,
    mv: null,
    move_error: null,
  }));
  return { revision: 1, root: 0, nodes };
}

const place = (view: VariationTreeView) => {
  const layout = layoutTree(view);
  return Object.fromEntries(layout.nodes.map((placed) => [placed.node.id, [placed.lane, placed.row]]));
};

test("first children run straight down the main lane", () => {
  const layout = layoutTree(tree([[0, null], [1, 0], [2, 1], [3, 2]]));
  expect(place(tree([[0, null], [1, 0], [2, 1], [3, 2]]))).toEqual({ 0: [0, 0], 1: [0, 1], 2: [0, 2], 3: [0, 3] });
  expect(layout.lanes).toBe(1);
  expect(layout.rows).toBe(4);
  expect(layout.edges.map(({ from, to }) => [from.node.id, to.node.id])).toEqual([[0, 1], [1, 2], [2, 3]]);
});

test("a variation starts one row below its parent, in the next lane", () => {
  // 0-1-2-3 main line; 4-5 branches from 1.
  expect(place(tree([[0, null], [1, 0], [2, 1], [3, 2], [4, 1], [5, 4]]))).toMatchObject({ 4: [1, 2], 5: [1, 3] });
});

test("a short variation frees its lane for a later one further down", () => {
  // Branch 5 from 1 (rows 2) and branch 6 from 3 (rows 4): lane 1 is reused.
  const placed = place(tree([[0, null], [1, 0], [2, 1], [3, 2], [4, 3], [5, 1], [6, 3]]));
  expect(placed[5]).toEqual([1, 2]);
  expect(placed[6]).toEqual([1, 4]);
});

test("overlapping variations take separate lanes", () => {
  // Branch 5-6-7 from 1 occupies rows 2..4 of lane 1; branch 8 from 2 starts at row 3.
  const placed = place(tree([[0, null], [1, 0], [2, 1], [3, 2], [4, 3], [5, 1], [6, 5], [7, 6], [8, 2]]));
  expect(placed[5]).toEqual([1, 2]);
  expect(placed[8]).toEqual([2, 3]);
});
