import type { MoveRank } from "@/api/backend";

/** How many of the latest moves get a number or a mark: 0 none, -1 all, otherwise the last N. */
export type MoveWindow = number;

export interface NumberRule {
  window: MoveWindow;
  /** 总是从1开始显示: the shown window counts from 1. */
  fromOne: boolean;
  /** Where the displayed line leaves the main line (that move's number); null on the main line. */
  branchPoint: number | null;
  /** 分支内从1开始显示: a variation's moves count from 1. */
  branchFromOne: boolean;
  /** 分支内总是显示所有手数. */
  allInBranch: boolean;
}

/**
 * The number written on each stone (`BoardRenderer.drawMoveNumbers`), in `numbers`' order; null
 * where none is written. `numbers` holds the number of the move that placed each stone.
 */
export function moveLabels(numbers: readonly (number | null | undefined)[], rule: NumberRule): (string | null)[] {
  const onBranch = rule.branchPoint !== null;
  const everything = rule.allInBranch && onBranch;
  const window = everything ? -1 : rule.window;
  if (window === 0) return numbers.map(() => null);
  const last = numbers.reduce<number>((highest, number) => Math.max(highest, number ?? 0), 0);
  const shift = window > 0 && rule.fromOne && last > window ? last - window : 0;
  return numbers.map((number) => {
    if (!number || number <= 0) return null;
    if (window > 0 && last - number >= window) return null;
    if (shift > 0) return String(number - shift);
    if (onBranch && rule.branchFromOne && number > rule.branchPoint!) return String(number - rule.branchPoint!);
    return String(number);
  });
}

/** Java's `boardMarkRadiusFactor`: a worse move gets a bigger dot, as a share of a cell. */
export const RANK_MARK_RADIUS: Readonly<Record<MoveRank, number>> = {
  Best: 0.1,
  Good: 0.1,
  Normal: 0.1225,
  Inaccuracy: 0.145,
  Mistake: 0.1675,
  Blunder: 0.19,
};

export interface RankedStone {
  index: number;
  rank: MoveRank;
}

/**
 * The stones that get a rank dot (`BoardRenderer.drawMoveRankMark`): those still standing from the
 * last `window` moves (all for -1, none for 0) whose move has a rank. The last move is left out:
 * it keeps its own mark.
 */
export function rankedStones(
  numbers: readonly (number | null | undefined)[],
  nodes: readonly (number | null | undefined)[],
  ranks: ReadonlyMap<number, MoveRank>,
  window: MoveWindow,
): RankedStone[] {
  if (window === 0) return [];
  const last = numbers.reduce<number>((highest, number) => Math.max(highest, number ?? 0), 0);
  const marked: RankedStone[] = [];
  numbers.forEach((number, index) => {
    const node = nodes[index];
    if (!number || node === null || node === undefined || number === last) return;
    if (window > 0 && last - number >= window) return;
    const rank = ranks.get(node);
    if (rank) marked.push({ index, rank });
  });
  return marked;
}
