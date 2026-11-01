import { useCallback, useEffect, useRef, useState } from "react";
import type { Backend, BenchmarkStatus } from "@/api/backend";

type BenchmarkBackend = Pick<
  Backend,
  "subscribeBenchmarkProgress" | "unsubscribe" | "benchmarkStatus" | "startBenchmark" | "cancelBenchmark" | "resetBenchmark"
>;

export interface BenchmarkControls {
  status: BenchmarkStatus | null;
  start(): void;
  cancel(): void;
  reset(): void;
}

function report(error: unknown) {
  console.error("KataGo tuning command failed", error);
}

/**
 * The tuning run as Rust reports it. Subscribes before reading the current state; as with
 * whole-game progress, a command's reply loses to any event that arrived after it was sent.
 */
export function useBenchmark(backend: BenchmarkBackend): BenchmarkControls {
  const [status, setStatus] = useState<BenchmarkStatus | null>(null);
  const events = useRef(0);

  useEffect(() => {
    let active = true;
    let heard = false;
    let id: number | undefined;
    backend
      .subscribeBenchmarkProgress((next) => {
        heard = true;
        events.current += 1;
        if (active) setStatus(next);
      })
      .then((subscription) => {
        id = subscription;
        if (!active) {
          void backend.unsubscribe(subscription);
          return null;
        }
        return backend.benchmarkStatus();
      })
      .then((initial) => {
        if (active && initial && !heard) setStatus(initial);
      })
      .catch(report);
    return () => {
      active = false;
      if (id !== undefined) void backend.unsubscribe(id);
    };
  }, [backend]);

  const apply = useCallback((send: () => Promise<BenchmarkStatus>) => {
    const seen = events.current;
    send()
      .then((reply) => {
        if (events.current === seen) setStatus(reply);
      })
      .catch(report);
  }, []);

  return {
    status,
    start: () => apply(() => backend.startBenchmark()),
    cancel: () => apply(() => backend.cancelBenchmark()),
    reset: () => apply(() => backend.resetBenchmark()),
  };
}
