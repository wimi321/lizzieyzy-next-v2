import { useCallback, useEffect, useRef, useState } from "react";
import type { AnalysisState, Backend } from "@/api/backend";

type RunBackend = Pick<Backend, "subscribeAnalysisState" | "unsubscribe" | "analysisState" | "togglePondering">;

function report(error: unknown) {
  console.error("Analysis state command failed", error);
}

/**
 * Whether live analysis may run now and why it is paused: by the user or, as LizzieYzy Next does,
 * because the position reached its time or visit limit. Subscribes before reading; a toggle's reply
 * loses to any event that arrived after it was sent.
 */
export function useAnalysisRun(backend: RunBackend): { state: AnalysisState | null; toggle(): void } {
  const [state, setState] = useState<AnalysisState | null>(null);
  const events = useRef(0);

  useEffect(() => {
    let active = true;
    let heard = false;
    let id: number | undefined;
    backend
      .subscribeAnalysisState((next) => {
        heard = true;
        events.current += 1;
        if (active) setState(next);
      })
      .then((subscription) => {
        id = subscription;
        if (!active) {
          void backend.unsubscribe(subscription);
          return null;
        }
        return backend.analysisState();
      })
      .then((initial) => {
        if (active && initial && !heard) setState(initial);
      })
      .catch(report);
    return () => {
      active = false;
      if (id !== undefined) void backend.unsubscribe(id);
    };
  }, [backend]);

  const toggle = useCallback(() => {
    const seen = events.current;
    backend
      .togglePondering()
      .then((reply) => {
        if (events.current === seen) setState(reply);
      })
      .catch(report);
  }, [backend]);

  return { state, toggle };
}
