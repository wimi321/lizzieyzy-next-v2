import { type WheelEvent, useRef } from "react";

/** A wheel notch (or this much trackpad travel) moves one move, as on Java's board. */
const WHEEL_STEP = 40;

/** The board's wheel steps through the game: a notch, or trackpad travel adding up to one, a move. */
export function useWheelSteps(step: (steps: number) => void) {
  const wheel = useRef(0);
  return (event: WheelEvent<HTMLElement>) => {
    // Lines (Firefox, some mice) count as whole notches.
    wheel.current += event.deltaMode === 0 ? event.deltaY : Math.sign(event.deltaY) * WHEEL_STEP;
    if (Math.abs(wheel.current) < WHEEL_STEP) return;
    const steps = Math.sign(wheel.current);
    wheel.current = 0;
    step(steps);
  };
}
