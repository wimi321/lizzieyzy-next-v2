import { useCallback, useEffect, useRef, useState } from "react";

/** Live analysis can arrive 20 times a second; review views need far fewer reads. */
const LIVE_REFRESH_MS = 800;

export interface ReviewSignals {
  /** Document revision: a new game, edits or settings changes. */
  revision: number;
  /** Whole-game progress count; each finished position adds results. */
  progress: number;
  /** Visits of the live analysis on the current node. */
  live: number;
}

function report(error: unknown) {
  console.error("Review query failed", error);
}

/**
 * Re-reads a derived view from Rust when the document or whole-game progress
 * changes, and at most every LIVE_REFRESH_MS while live analysis runs. Only
 * the newest request's reply is shown. `read` must be stable (useCallback).
 */
export function useLiveQuery<T>(read: () => Promise<T>, signals: ReviewSignals): T | null {
  const [view, setView] = useState<T | null>(null);
  const latest = useRef(0);
  const pending = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refresh = useCallback(() => {
    const request = ++latest.current;
    read()
      .then((next) => {
        if (request === latest.current) setView(next);
      })
      .catch(report);
  }, [read]);

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
