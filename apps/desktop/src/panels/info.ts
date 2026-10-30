import type { AnalysisSummary, GraphPointView, MoveView } from "@/api/backend";

/** Change caused by the last move, from the mover's side: winrate in percentage points, score in points. */
export interface MoveChange {
  moveNumber: number;
  winrate: number;
  score: number;
}

/**
 * Compares the current node with its parent on the displayed line. Summaries
 * are Black's perspective; the change is flipped for a White move so that a
 * negative number always means the move lost ground, as in LizzieYzy Next.
 */
export function lastMoveChange(
  points: readonly GraphPointView[],
  currentIndex: number,
  lastMove: MoveView | null,
): MoveChange | null {
  const current = points[currentIndex];
  const previous = points[currentIndex - 1]?.summary;
  if (!current?.summary || !previous || !lastMove) return null;
  const sign = lastMove.data.color === "Black" ? 1 : -1;
  return {
    moveNumber: current.move_number,
    winrate: sign * (current.summary.winrate - previous.winrate) * 100,
    score: sign * (current.summary.score_lead - previous.score_lead),
  };
}

export type Lead = { side: "Black" | "White"; points: number } | { side: "Even" };

/** Who leads by how much, rounded the way it is shown. */
export function leadOf(summary: AnalysisSummary): Lead {
  const points = Math.round(Math.abs(summary.score_lead) * 10) / 10;
  if (points === 0) return { side: "Even" };
  return { side: summary.score_lead > 0 ? "Black" : "White", points };
}

/** Signed, one decimal, with a real minus sign: "+0.3", "−1.2", "0.0". */
export function signed(value: number): string {
  const rounded = Math.round(value * 10) / 10;
  if (rounded === 0) return "0.0";
  return `${rounded > 0 ? "+" : "−"}${Math.abs(rounded).toFixed(1)}`;
}
