import type { GraphPointView } from "@/api/backend";

/**
 * Java's `goToMoveNumberBeyondBranch` (the 跳转 field): move `n` on the displayed line; past the
 * end of a branch, move `n` of the main line. Null when neither line is that long.
 */
export function jumpTarget(line: readonly GraphPointView[], mainLine: readonly number[], n: number): number | null {
  const onLine = line.find((point) => point.move_number === n);
  if (onLine) return onLine.node_id;
  return n >= 0 && n < mainLine.length ? (mainLine[n] ?? null) : null;
}
