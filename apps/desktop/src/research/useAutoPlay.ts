import { useEffect, useRef, useState } from "react";

export interface AutoPlay {
  playing: boolean;
  start(intervalMs: number): void;
  stop(): void;
}

/**
 * 自动播放 on the main board: one move forward every interval, stopping at the branch's last move.
 * A move the user makes meanwhile takes that beat; playing goes on from there at the next one.
 */
export function useAutoPlay(step: (steps: number) => void, current: number | null, atEnd: boolean): AutoPlay {
  const [period, setPeriod] = useState<number | null>(null);
  // At the last move there is nothing more to play.
  if (period !== null && atEnd) setPeriod(null);

  const latest = useRef({ step, current, atEnd });
  useEffect(() => {
    latest.current = { step, current, atEnd };
  });
  // The position playing goes on from, and whether a step of its own is on the way.
  const from = useRef<number | null>(null);
  const stepping = useRef(false);
  useEffect(() => {
    if (!stepping.current) return;
    stepping.current = false;
    from.current = current;
  }, [current]);

  useEffect(() => {
    if (period === null) return;
    from.current = latest.current.current;
    stepping.current = false;
    const timer = window.setInterval(() => {
      const now = latest.current;
      if (now.current !== from.current) {
        from.current = now.current;
        return;
      }
      if (now.atEnd) return;
      stepping.current = true;
      now.step(1);
    }, period);
    return () => window.clearInterval(timer);
  }, [period]);

  return { playing: period !== null, start: setPeriod, stop: () => setPeriod(null) };
}
