import { useCallback, useEffect, useRef, useState } from "react";
import type { Backend, BoardSyncOptions, BoardSyncStatus } from "@/api/backend";

type SyncBackend = Pick<Backend, "subscribeBoardSyncStatus" | "unsubscribe" | "boardSyncStatus" | "startBoardSync" | "stopBoardSync">;

export interface BoardSyncControls {
  status: BoardSyncStatus | null;
  start(options: BoardSyncOptions): void;
  stop(): void;
}

function report(error: unknown) {
  console.error("Board synchronization command failed", error);
}

/**
 * readboard's synchronization as Rust reports it. Subscribes before reading the current state,
 * and as with whole-game progress, a command's reply loses to any event that arrived after it
 * was sent. Only Windows starts it; elsewhere the status simply stays Stopped.
 */
export function useBoardSync(backend: SyncBackend): BoardSyncControls {
  const [status, setStatus] = useState<BoardSyncStatus | null>(null);
  const events = useRef(0);

  useEffect(() => {
    let active = true;
    let heard = false;
    let id: number | undefined;
    backend
      .subscribeBoardSyncStatus((next) => {
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
        return backend.boardSyncStatus();
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

  const apply = useCallback((send: () => Promise<BoardSyncStatus>) => {
    const seen = events.current;
    send()
      .then((reply) => {
        if (events.current === seen) setStatus(reply);
      })
      .catch(report);
  }, []);

  return {
    status,
    start: (options) => apply(() => backend.startBoardSync(options)),
    stop: () => apply(() => backend.stopBoardSync()),
  };
}
