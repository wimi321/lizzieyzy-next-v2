import type { VariationNodeView, VariationTreeView } from "@/api/backend";

/**
 * Branch navigation as Java's `Board.previousBranch` / `nextBranch` and
 * `LizzieFrame.moveToMainTrunk` do it, over the tree the backend sends. Only the target node
 * is chosen here; moving there is an ordinary `goTo`.
 */
export function branchTarget(tree: VariationTreeView, current: number, direction: -1 | 1): number | null {
  const nodes = byId(tree);
  if (!nodes.has(current)) return null;
  const candidates = branchCandidates(tree, nodes, depthOf(nodes, current));
  const index = candidates.indexOf(current);
  const target = index < 0 ? undefined : candidates[index + direction];
  return target === undefined || target === current ? null : target;
}

/** The deepest ancestor on the main line (first children from the root), unless already on it. */
export function mainTrunkTarget(tree: VariationTreeView, current: number): number | null {
  const nodes = byId(tree);
  const trunk = new Set(mainTrunk(tree, nodes));
  let node = nodes.get(current);
  while (node && !trunk.has(node.id)) node = node.parent === null ? undefined : nodes.get(node.parent);
  return node && node.id !== current ? node.id : null;
}

function byId(tree: VariationTreeView): Map<number, VariationNodeView> {
  return new Map(tree.nodes.map((node) => [node.id, node]));
}

function depthOf(nodes: Map<number, VariationNodeView>, id: number): number {
  let depth = 0;
  let node = nodes.get(id);
  while (node && node.parent !== null) {
    depth += 1;
    node = nodes.get(node.parent);
  }
  return depth;
}

function mainTrunk(tree: VariationTreeView, nodes: Map<number, VariationNodeView>): number[] {
  const line: number[] = [];
  let id: number | undefined = tree.root;
  while (id !== undefined) {
    line.push(id);
    id = nodes.get(id)?.children[0];
  }
  return line;
}

/**
 * Every node at this depth, left to right. When the main line is shorter, its last node leads
 * the list, so moving left from a deep variation can return to the main line.
 */
function branchCandidates(tree: VariationTreeView, nodes: Map<number, VariationNodeView>, depth: number): number[] {
  let level = [tree.root];
  for (let i = 0; i < depth; i += 1) level = level.flatMap((id) => nodes.get(id)?.children ?? []);
  const trunk = mainTrunk(tree, nodes);
  const first = level[0];
  if (first === undefined || !trunk.includes(first)) {
    const end = trunk[trunk.length - 1];
    if (end !== undefined) level.unshift(end);
  }
  return level;
}
