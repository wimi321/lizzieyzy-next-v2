import type { VariationNodeView, VariationTreeView } from "@/api/backend";

export interface PlacedNode {
  node: VariationNodeView;
  /** Column: 0 is the main line, variations to the right. */
  lane: number;
  /** Depth from the root, one row per node. */
  row: number;
}

export interface TreeLayout {
  nodes: PlacedNode[];
  byId: Map<number, PlacedNode>;
  edges: Array<{ from: PlacedNode; to: PlacedNode }>;
  lanes: number;
  rows: number;
}

/**
 * Lays the game tree out like LizzieYzy Next's variation tree: every chain of
 * first children runs straight down one lane; each other child starts a chain
 * in the leftmost lane right of its parent that is free for the rows it needs,
 * so short variations reuse space and the tree stays narrow.
 */
export function layoutTree(tree: VariationTreeView): TreeLayout {
  const nodesById = new Map(tree.nodes.map((node) => [node.id, node]));
  const byId = new Map<number, PlacedNode>();
  const edges: TreeLayout["edges"] = [];
  /** Last occupied row per lane. */
  const laneEnd: number[] = [];
  let rows = 0;

  const chainLength = (start: VariationNodeView): number => {
    let length = 1;
    let node = start;
    for (let child = nodesById.get(node.children[0] ?? -1); child; child = nodesById.get(node.children[0] ?? -1)) {
      length++;
      node = child;
    }
    return length;
  };

  const freeLane = (after: number, from: number, to: number): number => {
    let lane = after + 1;
    // A lane is free if its last stone sits at least one row above the branch's
    // first, leaving room for the connector.
    while ((laneEnd[lane] ?? -2) >= from - 1) lane++;
    laneEnd[lane] = to;
    return lane;
  };

  const placeChain = (start: VariationNodeView, lane: number, row: number, parent: PlacedNode | null) => {
    const branches: Array<{ child: VariationNodeView; parent: PlacedNode }> = [];
    let previous = parent;
    let current: VariationNodeView | undefined = start;
    let depth = row;
    while (current) {
      const placed: PlacedNode = { node: current, lane, row: depth };
      byId.set(current.id, placed);
      if (previous) edges.push({ from: previous, to: placed });
      rows = Math.max(rows, depth + 1);
      for (const id of current.children.slice(1)) {
        const child = nodesById.get(id);
        if (child) branches.push({ child, parent: placed });
      }
      previous = placed;
      current = nodesById.get(current.children[0] ?? -1);
      depth++;
    }
    laneEnd[lane] = Math.max(laneEnd[lane] ?? -2, depth - 1);
    for (const branch of branches) {
      const first = branch.parent.row + 1;
      const branchLane = freeLane(branch.parent.lane, first, first + chainLength(branch.child) - 1);
      placeChain(branch.child, branchLane, first, branch.parent);
    }
  };

  const root = nodesById.get(tree.root);
  if (root) placeChain(root, 0, 0, null);
  const nodes = [...byId.values()];
  return { nodes, byId, edges, lanes: laneEnd.length, rows };
}
