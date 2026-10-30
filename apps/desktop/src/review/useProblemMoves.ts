import { useCallback, useEffect, useRef, useState } from "react";
import type { Backend, ProblemMovesQuery, ProblemMovesView } from "@/api/backend";

/** Live analysis can arrive 20 times a second; the list needs far fewer reads. */
const LIVE_REFRESH_MS = 800;

export interface ProblemSignals {
  /** Document revision: a new game, edits or settings changes. */
  revision: number;
  /** Whole-game progress count; each finished position can add losses. */
  progress: number;
  /** Visits of the live analysis on the current node. */
  live: number;
}

function report(error: unknown) {
  console.error("Problem moves request failed", error);
}

/**
 * Reads problem moves from Rust when the document or whole-game progress
 * changes, and at most every LIVE_REFRESH_MS while live analysis runs. Only
 * the newest request's reply is shown.
 */
export function useProblemMoves(
  backend: Pick<Backend, "problemMoves">,
  query: ProblemMovesQuery,
  signals: ProblemSignals,
): ProblemMovesView | null {
  const [view, setView] = useState<ProblemMovesView | null>(null);
  const latest = useRef(0);
  const pending = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refresh = useCallback(() => {
    const request = ++latest.current;
    backend
      .problemMoves(query)
      .then((next) => {
        if (request === latest.current) setView(next);
      })
      .catch(report);
  }, [backend, query]);

  useEffect(() => {
    refresh();
  }, [refresh, signals.revision, signals.progress]);

  useEffect(() => {
    if (pending.current !== null) return;
    pending.current = setTimeout(() => {
      pending.current = null;
      refresh();
    }, LIVE_REFRESH_MS);
  }, [refresh, signals.live]);

  useEffect(
    () => () => {
      if (pending.current !== null) clearTimeout(pending.current);
    },
    [],
  );

  return view;
}
