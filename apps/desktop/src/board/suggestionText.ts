import type { ColorView } from "@/api/backend";
import { type Context, NUMBER_FONT, text, UI_FONT } from "./canvas";
import type { Geometry } from "./geometry";
import { playoutsText, scoreText, winrateText } from "./suggestion";

/** 显示 → 选点信息 and 目数显示, as the board needs them. */
export interface SuggestionStyle {
  winrate: boolean;
  visits: boolean;
  score: boolean;
  /** 显示目数为差值: each score against the best move's. */
  scoreAsDiff: boolean;
  /** The engine's order in the orange tag. */
  order: boolean;
  /** 总是显示黑方胜率. */
  blackWinrate: boolean;
  /** KataGo's lead counts komi; off, the board score is shown instead. */
  scoreWithKomi: boolean;
  komi: number;
}

export const DEFAULT_SUGGESTION_STYLE: SuggestionStyle = {
  winrate: true,
  visits: true,
  score: true,
  scoreAsDiff: false,
  order: true,
  blackWinrate: false,
  scoreWithKomi: true,
  komi: 0,
};

/** `Utils.convertScoreToString`: a mover's lead as written, against the best lead or without komi. */
export function scoreLabel(lead: number, best: number, toPlay: ColorView, style: SuggestionStyle): string {
  if (style.scoreAsDiff) {
    const diff = Math.round((lead - best) * 10) / 10;
    return diff > 0 ? `+${diff.toFixed(1)}` : diff < 0 ? `-${Math.abs(diff).toFixed(1)}` : "0.0";
  }
  if (style.scoreWithKomi) return scoreText(lead);
  return scoreText(toPlay === "Black" ? lead + style.komi : lead - style.komi);
}

export interface SuggestionValues {
  /** The mover's winrate, 0–1. */
  winrate: number;
  /** Black's winrate, 0–1. */
  blackWinrate: number;
  visits: number;
  score: string;
}

/** `drawSuggestionText`: the shown rows of a candidate, in Java's three-, two- or one-row layout. */
export function drawSuggestionText(context: Context, geometry: Geometry, x: number, y: number, values: SuggestionValues, style: SuggestionStyle) {
  const { cell, stoneRadius: r } = geometry;
  const shownWinrate = style.blackWinrate ? values.blackWinrate : values.winrate;
  const texts = [winrateText(shownWinrate), playoutsText(values.visits), values.score];
  const fields = [style.winrate ? 0 : -1, style.visits ? 1 : -1, style.score ? 2 : -1].filter((field) => field >= 0);
  const font = (field: number) => (field === 1 ? UI_FONT : NUMBER_FONT);
  const narrow = shownWinrate * 100 < 10;

  if (fields.length === 3) {
    text(context, texts[0]!, x, y - Math.round(cell * (narrow ? 0.127 : 0.125)), NUMBER_FONT, cell * (narrow ? 0.36 : 0.35), cell * 0.67, "above");
    text(context, texts[1]!, x, y + Math.round(cell * 0.18), UI_FONT, cell * 0.34, values.visits < 1000 ? r * 1.3 : r * 1.8, "above");
    text(context, texts[2]!, x, y + Math.round(cell * 0.435), NUMBER_FONT, (geometry.available * 0.273) / (geometry.columns - 1), r * 1.6, "above");
  } else if (fields.length === 2) {
    const [first, second] = fields as [number, number];
    // The upper row sits on its baseline just above the middle.
    if (first === 0) {
      text(context, texts[0]!, x, y - Math.trunc(cell / (narrow ? 15 : 16)), font(0), r, cell * (narrow ? 0.57 : 0.735), "above");
    } else {
      text(context, texts[1]!, x, y - Math.trunc(r / 15), font(1), r * 0.82, r * 1.73, "above");
    }
    const below = second === 2 ? Math.trunc((r * 4) / 9) : Math.trunc((r * 15) / 35);
    text(context, texts[second]!, x, y + below, font(second), second === 2 ? r * 0.75 : r * 0.77, second === 2 ? r * 1.6 : r * 1.8);
  } else if (fields.length === 1) {
    const field = fields[0]!;
    text(context, texts[field]!, x, y, font(field), field === 0 ? cell * 0.46 : r, field === 2 ? r * 1.7 : r * 1.9);
  }
}
