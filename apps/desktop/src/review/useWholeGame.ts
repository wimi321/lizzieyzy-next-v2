import { useCallback, useEffect, useRef, useState } from "react";
import type { Backend, WholeGameProgress } from "@/api/backend";

type ProgressBackend = Pick<
  Backend,
  | "subscribeWholeGameProgress"
  | "unsubscribe"
  | "wholeGameProgress"
  | "startWholeGameAnalysis"
  | "pauseWholeGameAnalysis"
  | "resumeWholeGameAnalysis"
  | "cancelWholeGameAnalysis"
>;

export interface WholeGameControls {
  progress: WholeGameProgress | null;
  start(visits: number): void;
  pause(): void;
  resume(): void;
  cancel(): void;
}

function report(error: unknown) {
  console.error("Whole-game analysis command failed", error);
}

/**
 * The whole-game job as Rust reports it. Subscribes before reading the
 * current state, as the backend asks. A command's reply is shown at once
 * unless a progress event arrived after the command was sent: the event is
 * then the newer truth and the reply would move the display backwards.
 */
export function useWholeGame(backend: ProgressBackend): WholeGameControls {
  const [progress, setProgress] = useState<WholeGameProgress | null>(null);
  const events = useRef(0);

  useEffect(() => {
    let active = true;
    let heard = false;
    let id: number | undefined;
    backend
      .subscribeWholeGameProgress((next) => {
        heard = true;
        events.current += 1;
        if (active) setProgress(next);
      })
      .then((subscription) => {
        id = subscription;
        if (!active) {
          void backend.unsubscribe(subscription);
          return null;
        }
        return backend.wholeGameProgress();
      })
      .then((initial) => {
        if (active && initial && !heard) setProgress(initial);
      })
      .catch(report);
    return () => {
      active = false;
      if (id !== undefined) void backend.unsubscribe(id);
    };
  }, [backend]);

  const apply = useCallback((send: () => Promise<WholeGameProgress>) => {
    const seen = events.current;
    send()
      .then((reply) => {
        if (events.current === seen) setProgress(reply);
      })
      .catch(report);
  }, []);

  return {
    progress,
    start: (visits) => apply(() => backend.startWholeGameAnalysis({ visits })),
    pause: () => apply(() => backend.pauseWholeGameAnalysis()),
    resume: () => apply(() => backend.resumeWholeGameAnalysis()),
    cancel: () => apply(() => backend.cancelWholeGameAnalysis()),
  };
}
