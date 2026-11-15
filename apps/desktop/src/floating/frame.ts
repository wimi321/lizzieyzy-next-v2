import { type MouseEvent, type PointerEvent, useEffect, useRef } from "react";
import type { Backend, ResizeEdge } from "@/api/backend";

/** How far the pointer moves with the button down before a press becomes a window drag. */
export const DRAG_THRESHOLD = 4;
/** A square window is set this long after the last resize, so the system's drag is not fought. */
export const SQUARE_DELAY = 150;

export const EDGES: readonly ResizeEdge[] = ["North", "South", "East", "West", "NorthEast", "NorthWest", "SouthEast", "SouthWest"];

type FrameBackend = Pick<Backend, "dragWindow" | "setWindowSquare">;

function report(error: unknown) {
  console.error("Floating window failed", error);
}

/**
 * A floating board's window, as Java's `IndependentMainBoard`: a press that moves becomes a window
 * drag (and plays nothing); the window keeps square after a resize. Locked, neither happens.
 */
export function useFloatingFrame(backend: FrameBackend, locked: boolean) {
  const press = useRef<{ x: number; y: number; dragged: boolean } | null>(null);
  const swallow = useRef(false);

  useEffect(() => {
    let timer: number | undefined;
    const onResize = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        const side = Math.min(window.innerWidth, window.innerHeight);
        if (Math.abs(window.innerWidth - window.innerHeight) > 1) void backend.setWindowSquare(side).catch(report);
      }, SQUARE_DELAY);
    };
    window.addEventListener("resize", onResize);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("resize", onResize);
    };
  }, [backend]);

  return {
    onPointerDownCapture: (event: PointerEvent<HTMLElement>) => {
      swallow.current = false;
      // Alt+drag frames points on the board; buttons and edges do their own thing.
      const target = event.target as Element;
      press.current = locked || event.button !== 0 || event.altKey || target.closest("button, .floating-edge") ? null : { x: event.clientX, y: event.clientY, dragged: false };
    },
    onPointerMoveCapture: (event: PointerEvent<HTMLElement>) => {
      const start = press.current;
      if (!start || start.dragged || (event.buttons & 1) === 0) return;
      if (Math.hypot(event.clientX - start.x, event.clientY - start.y) < DRAG_THRESHOLD) return;
      start.dragged = true;
      swallow.current = true;
      void backend.dragWindow().catch(report);
    },
    onPointerUpCapture: () => {
      press.current = null;
    },
    // The click that ends a drag is the drag's, not a move.
    onClickCapture: (event: MouseEvent<HTMLElement>) => {
      if (!swallow.current) return;
      swallow.current = false;
      event.stopPropagation();
      event.preventDefault();
    },
  };
}
