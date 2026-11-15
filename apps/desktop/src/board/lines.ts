import type { AnalysisView, CandidateView, VariationNodeView, VariationTreeView } from "@/api/backend";
import type { NextMove } from "./draw";

/**
 * The candidates of this position that have a move and a line, best first (the engine's order):
 * the first is the sub-board's line, 四方图 shows the first four.
 */
export function rankedLines(analysis: AnalysisView | null, node: number | null): CandidateView[] {
  if (!analysis || analysis.node_id !== node) return [];
  return analysis.candidates.filter((candidate) => candidate.point && candidate.pv.length > 0).sort((a, b) => a.order - b.order);
}

/** The recorded continuations of the current node, each with the position it leads to when analysed. */
export function nextMovesOf(
  tree: VariationTreeView | null,
  current: VariationNodeView | null,
  points: readonly { node_id: number; summary: NextMove["after"] }[],
): NextMove[] {
  if (!tree || !current) return [];
  const moves: NextMove[] = [];
  for (const id of current.children) {
    const mv = tree.nodes.find((node) => node.id === id)?.mv;
    if (mv?.type !== "Play") continue;
    moves.push({ point: mv.data.point, after: points.find((point) => point.node_id === id)?.summary ?? null });
  }
  return moves;
}
