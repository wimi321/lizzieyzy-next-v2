import type { PhaseEstimate, StrengthPhasesReport } from "@/api/backend";

/**
 * Java's `StrengthPhases`: where the phases start out, the shortest a phase can be dragged to,
 * and how far apart two ranks must be to call one side better. Rust normalizes the bounds it
 * is sent; these only keep the dividers where Java's timeline lets them go.
 */
export const DEFAULT_BOUNDS: readonly [number, number] = [60, 150];
export const MIN_PHASE_MOVES = 10;
const EVEN = 0.3;
const CLEAR = 1.0;

export interface PhaseLabels {
  title: string;
  hint: string;
  reset: string;
  notReached: string;
  tooFew: string;
  even: string;
  slightly: string;
  clearly: string;
  best: string;
  weakest: string;
  divider: string;
  review: string;
  reviewBoth: string;
  reviewItem: string;
  reviewSame: string;
  steady: string;
  caveat: string;
}

/** Java's `StrengthPhaseCard.verdict`: who played the phase better, or why nobody can say. */
export function verdict(phase: PhaseEstimate, names: { black: string; white: string }, labels: PhaseLabels): string {
  if (!phase.played) return labels.notReached;
  const black = phase.black_ranked ? phase.black.rank_value : null;
  const white = phase.white_ranked ? phase.white.rank_value : null;
  if (black === null || white === null) return labels.tooFew;
  const lead = black - white;
  if (Math.abs(lead) < EVEN) return labels.even;
  return (Math.abs(lead) < CLEAR ? labels.slightly : labels.clearly).replace("{name}", lead > 0 ? names.black : names.white);
}

/**
 * Java's `StrengthPhaseView.review`: each side's weakest phase when it is clearly below its best,
 * that both played evenly when neither is, or nothing when too few phases are ranked.
 */
export function reviewLine(
  report: StrengthPhasesReport,
  names: { black: string; white: string },
  phaseNames: readonly string[],
  labels: PhaseLabels,
): string | null {
  const items: string[] = [];
  const weakest: number[] = [];
  let comparable = true;
  for (const side of ["black", "white"] as const) {
    comparable &&= report.phases.filter((phase) => phase[`${side}_ranked`]).length >= 2;
    const worst = report[side].worst;
    if (report[side].worth_reviewing && worst !== null) {
      weakest.push(worst);
      items.push(labels.reviewItem.replace("{name}", names[side]).replace("{phase}", phaseNames[worst] ?? ""));
    }
  }
  if (items.length === 0) return comparable ? labels.steady : null;
  if (weakest.length === 2 && weakest[0] === weakest[1]) return labels.reviewSame.replace("{phase}", phaseNames[weakest[0] ?? 0] ?? "");
  if (items.length === 1) return labels.review.replace("{item}", items[0] ?? "");
  return labels.reviewBoth.replace("{a}", items[0] ?? "").replace("{b}", items[1] ?? "");
}
