import type { FoxFailure, FoxResult } from "@/api/backend";

export interface FoxResultLabels {
  black: string;
  white: string;
  resignation: string;
  timeout: string;
  points: string;
  stones: string;
  win: string;
  other: string;
}

/** "黑中盘胜", "白胜 2.5 目", "黑超时胜": how LizzieYzy Next lists Fox results. */
export function resultText(result: FoxResult, labels: FoxResultLabels): string {
  if (!result.winner || result.kind === "Other") return labels.other;
  const side = result.winner === "Black" ? labels.black : labels.white;
  switch (result.kind) {
    case "Resignation":
      return labels.resignation.replace("{side}", side);
    case "Timeout":
      return labels.timeout.replace("{side}", side);
    case "Score":
      if (result.score === null) return labels.win.replace("{side}", side);
      return (result.unit === "Stones" ? labels.stones : labels.points)
        .replace("{side}", side)
        .replace("{score}", String(result.score));
    case "OtherWin":
      return labels.win.replace("{side}", side);
  }
}

/** Fox sends "2026-10-02 14:31:30"; the seconds add nothing in a list. */
export function shortTime(text: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2})/.exec(text);
  return match ? `${match[1]}-${match[2]}-${match[3]} ${match[4]}:${match[5]}` : text;
}

export type FoxErrorLabels = Record<FoxFailure["type"], string>;

/** Fox failures arrive typed from Rust; anything else is shown as its text. */
export function errorText(error: unknown, labels: FoxErrorLabels): string {
  if (typeof error === "object" && error !== null && "type" in error) {
    const typed = error as { type: string; data?: unknown };
    if (typed.type === "Fox" && typeof typed.data === "object" && typed.data !== null && "type" in typed.data) {
      const failure = typed.data as FoxFailure;
      const text = labels[failure.type];
      if (failure.type === "Http") return text.replace("{status}", String(failure.data.status));
      if (failure.type === "Api") return text.replace("{code}", String(failure.data.code));
      return text;
    }
    if (typed.type === "App" && typeof typed.data === "string") return typed.data;
  }
  return error instanceof Error ? error.message : String(error);
}
