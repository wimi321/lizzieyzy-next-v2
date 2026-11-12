import type { DocumentView, GameInfoEdit } from "@/api/backend";

/** The six rule presets by the names the rules engine knows them by. */
export const RULE_PRESETS = ["chinese", "japanese", "korean", "aga", "new-zealand", "tromp-taylor"] as const;

/** A field left as it was keeps its recorded value, even an explicit empty one; a cleared field is removed. */
export function editedText(original: string | null, text: string): string | null {
  if (text === (original ?? "")) return original;
  return text.trim() === "" ? null : text;
}

/** The game's information as it stands, with one change: every other field is sent back unchanged. */
export function infoEdit(document: DocumentView, change: Partial<GameInfoEdit>): GameInfoEdit {
  const info = document.game_info;
  return {
    black_name: info.black_name,
    black_rank: info.black_rank,
    white_name: info.white_name,
    white_rank: info.white_rank,
    komi: info.komi,
    rules: info.rules_preset ?? "chinese",
    result: info.result,
    date: info.date,
    event: info.event,
    ...change,
  };
}

/** What 保存 does: write back, ask before writing back, or ask where. */
export function saveRoute(path: string | null | undefined, confirmReplace: boolean): "write" | "confirm" | "choose" {
  if (!path) return "choose";
  return confirmReplace ? "confirm" : "write";
}
