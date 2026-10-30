import type { MoveRank } from "@/api/backend";

/** The six move tiers of the report's `tier_counts`, best first. */
export const TIER_ORDER: readonly MoveRank[] = ["Best", "Good", "Normal", "Inaccuracy", "Mistake", "Blunder"];

export interface BandLabels {
  beginner: string;
  ai: string;
  topPro: string;
  pro: string;
  kyuRange: string;
  kyuSingle: string;
  danRange: string;
  danSingle: string;
}

/**
 * Rust reports the rule-based band as LizzieYzy Next's estimator writes it
 * ("3-4d", "11-15k", "10d职业", "12d AI"); this only localises the wording.
 */
export function bandText(band: string, labels: BandLabels): string | null {
  const value = band.trim();
  if (value === "" || value === "-") return null;
  if (/^beginner$/i.test(value)) return labels.beginner;
  if (value.includes("AI")) return labels.ai;
  if (value.includes("一线职业")) return labels.topPro;
  if (value.includes("职业")) return labels.pro;
  const match = /^(\d+)(?:-(\d+))?([kd])$/.exec(value);
  if (!match) return value;
  const [, low = "", high, unit] = match;
  const template = unit === "k" ? (high ? labels.kyuRange : labels.kyuSingle) : high ? labels.danRange : labels.danSingle;
  return template.replace("{a}", low).replace("{b}", high ?? "");
}

/** Rates arrive as fractions; whole percentages read best in a summary. */
export function percent(rate: number): string {
  return `${Math.round(rate * 100)}%`;
}

/** Phase boundaries used by the Rust report, in move numbers. */
export const PHASES = [
  { key: "opening", first: 1, last: 60 },
  { key: "middlegame", first: 61, last: 160 },
  { key: "endgame", first: 161, last: null },
] as const;
