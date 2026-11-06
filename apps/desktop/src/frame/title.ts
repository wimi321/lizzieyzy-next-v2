/** Joins the non-empty parts, as Java's `MainWindowTitle.join` does. */
export function joinParts(separator: string, ...parts: (string | null | undefined)[]): string {
  return parts
    .map((part) => part?.trim() ?? "")
    .filter((part) => part.length > 0)
    .join(separator);
}

export interface TitleParts {
  app: string;
  /** "[33.6 -0.9 383]": Black's winrate, Black's lead and the visits. */
  evaluation?: string | null;
  engine?: string | null;
  /** "50 搜索/秒", while the engine searches. */
  speed?: string | null;
  /** "[黑] 甲 vs [白] 乙". */
  players?: string | null;
  /** "(结果:B+R)". */
  result?: string | null;
  file?: string | null;
}

/** "app - [evaluation] engine speed - players (result) - file", as Java's `MainWindowTitle`. */
export function windowTitle(parts: TitleParts): string {
  return joinParts(
    " - ",
    parts.app,
    joinParts(" ", parts.evaluation, parts.engine, parts.speed),
    joinParts(" ", parts.players, parts.result),
    parts.file,
  );
}

/** Java's `Utils.getPlayoutsString`: 383, 1.3k, 13k, 12.3m. */
export function playoutsText(playouts: number): string {
  if (playouts >= 10_000_000) return `${(Math.round(playouts / 100_000) / 10).toFixed(1)}m`;
  if (playouts >= 9950) return `${Math.round(playouts / 1000)}k`;
  if (playouts >= 1000) return `${(Math.round(playouts / 100) / 10).toFixed(1)}k`;
  return String(playouts);
}

/** "[33.6 -0.9 383]" from Black's winrate (0–1), Black's lead and the root visits. */
export function evaluationText(winrate: number, lead: number, visits: number): string {
  return `[${(winrate * 100).toFixed(1)} ${lead.toFixed(1)} ${playoutsText(visits)}]`;
}
