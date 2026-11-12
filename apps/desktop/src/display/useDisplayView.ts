import { useMemo } from "react";
import type { GameInfoView, GraphPointView, MoveRank } from "@/api/backend";
import type { StoneMarks } from "../board/Board";
import { type Display, hiddenRegions, numberWindow, rankWindow, shiftScores, suggestionStyle } from "./settings";

/**
 * What the 显示 settings mean for this game: the board's numbers and marks, the candidate text,
 * the next-move hint, the panels shown, and the score with or without komi.
 */
export function useDisplayView(
  shown: Display,
  info: GameInfoView | null,
  /** Where the displayed line's first move after the main line is; null on the main line. */
  branchStart: number | null,
  ranks: ReadonlyMap<number, MoveRank>,
  points: readonly GraphPointView[],
) {
  const marks = useMemo<StoneMarks>(
    () => ({
      numbers: {
        window: numberWindow(shown),
        fromOne: shown.move_numbers_from_one,
        branchPoint: branchStart === null ? null : branchStart - 1,
        branchFromOne: shown.branch_move_numbers_from_one,
        allInBranch: shown.show_all_branch_move_numbers,
      },
      rankWindow: rankWindow(shown),
      ranks,
    }),
    [shown, branchStart, ranks],
  );
  const komi = info?.komi ?? 0;
  const style = useMemo(() => suggestionStyle(shown, komi), [shown, komi]);
  const nextHint = useMemo(() => ({ mode: shown.next_moves, minVisits: shown.next_move_min_visits }), [shown.next_moves, shown.next_move_min_visits]);
  const hidden = useMemo(() => hiddenRegions(shown), [shown]);
  const scoreOffset = shown.score_lead_includes_komi ? 0 : komi;
  const scorePoints = useMemo(() => shiftScores(points, scoreOffset), [points, scoreOffset]);
  const names = useMemo(
    () => (shown.show_names_below_board && info && (info.black_name || info.white_name) ? { black: info.black_name ?? "", white: info.white_name ?? "" } : null),
    [info, shown.show_names_below_board],
  );
  return { marks, style, nextHint, hidden, scoreOffset, scorePoints, names };
}
