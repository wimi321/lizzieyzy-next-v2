import type { CandidateView, ColorView } from "@/api/backend";

/**
 * Backend values are Black's perspective. Candidate labels show the side to
 * move, as Lizzie does; flipping a number for display is not analysis logic.
 */
export function forMover(blackValue: number, toPlay: ColorView, kind: "winrate" | "score"): number {
  if (toPlay === "Black") return blackValue;
  return kind === "winrate" ? 1 - blackValue : -blackValue;
}

/** Winrate the mover gives up by choosing `candidate` instead of `best`, in percentage points. */
export function winrateLoss(candidate: CandidateView, best: CandidateView, toPlay: ColorView): number {
  const loss = forMover(best.winrate, toPlay, "winrate") - forMover(candidate.winrate, toPlay, "winrate");
  // Round to 0.01 so a loss that reads 6.0 on the labels is not tiered as 6.000000000000005.
  return Math.max(0, Math.round(loss * 10_000) / 100);
}

/** Tier 0 is the engine's top choice; tiers 1–5 grow with the winrate given up. */
export const TIER_LIMITS = [1, 3, 6, 12] as const;

export function candidateTier(lossPoints: number, isBest: boolean): number {
  if (isBest) return 0;
  const index = TIER_LIMITS.findIndex((limit) => lossPoints <= limit);
  return index === -1 ? TIER_LIMITS.length + 1 : index + 1;
}

export function formatWinrate(value: number): string {
  return (Math.min(1, Math.max(0, value)) * 100).toFixed(1);
}

export function formatVisits(visits: number): string {
  if (visits < 1000) return String(Math.max(0, Math.round(visits)));
  if (visits < 10_000) return `${trimZero((visits / 1000).toFixed(1))}k`;
  if (visits < 1_000_000) return `${Math.round(visits / 1000)}k`;
  if (visits < 10_000_000) return `${trimZero((visits / 1_000_000).toFixed(1))}m`;
  return `${Math.round(visits / 1_000_000)}m`;
}

export function formatScore(value: number): string {
  const rounded = Math.round(value * 10) / 10;
  if (rounded === 0) return "0.0";
  return `${rounded > 0 ? "+" : "−"}${Math.abs(rounded).toFixed(1)}`;
}

/** GTP column letters skip I. Boards wider than 25 fall back to numbers. */
export function columnLabel(x: number, columns: number): string {
  if (columns > 25) return String(x + 1);
  const letters = "ABCDEFGHJKLMNOPQRSTUVWXYZ";
  return letters[x] ?? String(x + 1);
}

/** Row 0 is the top line, labelled with the highest number as in GTP. */
export function rowLabel(y: number, rows: number): string {
  return String(rows - y);
}

function trimZero(text: string): string {
  return text.endsWith(".0") ? text.slice(0, -2) : text;
}
