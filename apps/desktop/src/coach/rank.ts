import type { MarkupView, MoveView } from "@/api/backend";
import type { CoachHint, CoachMoveRated, CoachRank, CoachSetup, CoachSummary, CoachVerdict } from "../api/coach";
import type { ColorView } from "../api/generated/ColorView";
import type en from "../locales/en.json";

export type CoachLabels = (typeof en)["coach"];

/** Java's 29 steps: 20 kyu up to 1 kyu, then 1 dan up to 9 dan. */
export const RANK_STEPS = 29;

export function rankAt(index: number): CoachRank {
  const step = Math.max(0, Math.min(RANK_STEPS - 1, Math.round(index)));
  return step < 20 ? { type: "Kyu", data: 20 - step } : { type: "Dan", data: step - 19 };
}

export function rankIndex(rank: CoachRank | undefined): number {
  if (!rank) return 22;
  return rank.type === "Kyu" ? 20 - rank.data : rank.data + 19;
}

export function rankName(rank: CoachRank | undefined, labels: CoachLabels): string {
  const shown = rank ?? rankAt(22);
  return (shown.type === "Kyu" ? labels.kyu : labels.dan).replace("{n}", String(shown.data));
}

/** Java's defaults: 3 dan, feedback as you play, a random colour, a natural pace, no handicap. */
export const COACH_DEFAULTS: Required<CoachSetup> = {
  rank: { type: "Dan", data: 3 },
  mode: "LiveFeedback",
  color: "Random",
  pace: "Natural",
  handicap: 0,
  from_current_position: false,
  width: 19,
  height: 19,
  auto_save: true,
};

/** Shown, not chosen: 0.5 with a handicap, 7.5 otherwise. */
export const komiFor = (handicap: number) => (handicap > 0 ? 0.5 : 7.5);

/** Java's verdicts as counted in the panel, from best to worst. */
export const VERDICTS: readonly CoachVerdict[] = ["Brilliant", "Good", "Slow", "Problem", "Blunder"];

/** The hint on the board: a "?" in the middle of one of nine areas, then a circle on the point. */
export function hintMarks(hint: CoachHint | null, width: number, height: number): MarkupView[] {
  if (!hint) return [];
  if (hint.type === "Region") {
    const centre = (part: number, size: number) => Math.min(size - 1, Math.floor(((part * 2 + 1) * size) / 6));
    return [{ point: { x: centre(hint.data.column ?? 1, width), y: centre(hint.data.row ?? 1, height) }, kind: "Letter", text: "?" }];
  }
  if (hint.type === "Exact" && hint.data.mv?.type === "Play") return [{ point: hint.data.mv.data.point, kind: "Circle", text: null }];
  return [];
}

/** "D16" for a move the engine suggests, as GTP writes it. */
export function moveText(move: MoveView | undefined, height: number, pass: string): string {
  if (!move || move.type !== "Play") return pass;
  const letters = "ABCDEFGHJKLMNOPQRSTUVWXYZ";
  const { x, y } = move.data.point;
  return `${letters[x] ?? "?"}${height - y}`;
}

export const percent = (value: number | null | undefined) => (value === null || value === undefined ? null : Math.round(value * 100));

/** A rated move's change in win chance, from the side that played it. */
export function changeFor(rating: CoachMoveRated): number | null {
  const change = rating.winrate_change;
  if (change === undefined) return null;
  return Math.round((rating.color === "White" ? -change : change) * 100);
}

/** 你赢了 / 你输了 / 和棋 / 对局结束 from the result and the human's colour. */
export function headline(summary: CoachSummary, human: ColorView | undefined, labels: CoachLabels, draw: string): string {
  const result = summary.result ?? "";
  if (result === "0") return draw;
  const winner = result.startsWith("B+") ? "Black" : result.startsWith("W+") ? "White" : null;
  if (!winner || !human) return labels.over;
  return winner === human ? labels.won : labels.lostGame;
}
