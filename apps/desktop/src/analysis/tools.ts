import type { RestrictionDepth, ScoringView, SearchParams } from "@/api/backend";
import { reasonOf } from "../everyday/useEveryday";
import type en from "../locales/en.json";

type Labels = typeof en.analysisTools;

/** Rust's default depth for both kinds of restriction (`Custom(5)`), and the range of 自定义. */
export const DEFAULT_DEPTH: RestrictionDepth = { type: "Custom", data: 5 };
export const CUSTOM_DEPTH = { min: 1, max: 200 };

/** The three search parameters: the KataGo setting, the range Rust accepts and KataGo's own value when unset. */
export const PARAMS = [
  { key: "playout_doubling_advantage", min: -3, max: 3, fallback: 0 },
  { key: "wide_root_noise", min: 0, max: 5, fallback: 0.04 },
  { key: "root_policy_temperature", min: 0.01, max: 100, fallback: 1 },
] as const;
export type ParamKey = (typeof PARAMS)[number]["key"];

export function paramRange(key: ParamKey) {
  return PARAMS.find((param) => param.key === key) ?? PARAMS[0];
}

/** A typed parameter: null when left empty (KataGo's own value), undefined when it is not a number in range. */
export function paramOf(text: string, key: ParamKey): number | null | undefined {
  const typed = text.trim().replace(",", ".");
  if (typed === "") return null;
  if (!/^[-+]?(\d+(\.\d*)?|\.\d+)$/.test(typed)) return undefined;
  const value = Number(typed);
  const { min, max } = paramRange(key);
  return value >= min && value <= max ? value : undefined;
}

/** None of the three set is KataGo's defaults, which Rust saves as no parameters at all. */
export function paramsOrNull(params: SearchParams): SearchParams | null {
  return PARAMS.some(({ key }) => params[key] !== null && params[key] !== undefined) ? params : null;
}

/** Whole numbers as they are, halves and the like with one decimal. */
export function scoreNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

export interface ScoreTerm {
  label: string;
  value: number;
}

/**
 * One side's sum under the rule, term by term from Rust's breakdown. Area: stones and territory;
 * territory: territory, prisoners and the opponent's dead stones. Komi and handicap compensation
 * show where they count.
 */
export function scoreTerms(view: ScoringView, side: "black" | "white", labels: Labels["scoring"]["terms"]): ScoreTerm[] {
  const own = view[side];
  const other = view[side === "black" ? "white" : "black"];
  const terms: ScoreTerm[] =
    view.rule === "Area"
      ? [
          { label: labels.living, value: own.living_stones },
          { label: labels.territory, value: own.territory },
        ]
      : [
          { label: labels.territory, value: own.territory },
          { label: labels.captures, value: own.captures },
          { label: labels.dead, value: other.dead_stones },
        ];
  if (own.komi !== 0) terms.push({ label: labels.komi, value: own.komi });
  if (own.handicap_compensation !== 0) terms.push({ label: labels.handicap, value: own.handicap_compensation });
  return terms;
}

/** Rust's `B+3.5` / `W+3.5` / `0` in the interface language. */
export function resultText(result: string, labels: Labels["scoring"]): string {
  const found = /^([BW])\+(.+)$/.exec(result);
  if (!found) return labels.draw;
  return (found[1] === "B" ? labels.blackWins : labels.whiteWins).replace("{n}", found[2] ?? "");
}

type Typed = { type: string; data?: unknown };

function typed(value: unknown): Typed | null {
  return typeof value === "object" && value !== null && "type" in value ? (value as Typed) : null;
}

/** An `AnalysisToolsError` or `ScoringError`, bare or inside its `CommandError`, in the interface language. */
export function toolsError(error: unknown, labels: Labels["errors"]): string {
  let failure = typed(error);
  if (failure && (failure.type === "AnalysisTools" || failure.type === "Scoring")) failure = typed(failure.data);
  const text = failure ? (labels as Record<string, string>)[failure.type] : undefined;
  if (!failure || text === undefined) return reasonOf(error);
  return text.replace("{field}", typeof failure.data === "string" ? failure.data : "");
}
