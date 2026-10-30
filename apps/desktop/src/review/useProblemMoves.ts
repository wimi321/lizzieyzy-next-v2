import { useCallback } from "react";
import type { Backend, ProblemMovesQuery, ProblemMovesView } from "@/api/backend";
import { type ReviewSignals, useLiveQuery } from "./useLiveQuery";

export type ProblemSignals = ReviewSignals;

/** Problem moves for the displayed line, refreshed as analysis arrives. */
export function useProblemMoves(
  backend: Pick<Backend, "problemMoves">,
  query: ProblemMovesQuery,
  signals: ReviewSignals,
): ProblemMovesView | null {
  const read = useCallback(() => backend.problemMoves(query), [backend, query]);
  return useLiveQuery(read, signals);
}
