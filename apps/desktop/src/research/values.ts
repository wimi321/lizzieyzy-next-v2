import type { ControlPanelBounds } from "@/api/backend";

/** Rust keeps the interval between 0.1 and 60 seconds (`autoplay_interval_ms`). */
export const INTERVAL_MS = { min: 100, max: 60_000 };

/** Seconds as typed, with a decimal point or comma, to whole milliseconds; null when out of range. */
export function intervalOf(text: string): number | null {
  const typed = text.trim().replace(",", ".");
  if (!/^\d+(\.\d*)?$|^\.\d+$/.test(typed)) return null;
  const ms = Math.round(Number(typed) * 1000);
  return ms >= INTERVAL_MS.min && ms <= INTERVAL_MS.max ? ms : null;
}

/** Java opens the panel at 500 × 460, centred on the window. */
const DEFAULT = { width: 500, height: 460 };
const MIN = { width: 220, height: 200 };
/** How much of the panel stays on screen, so it can always be dragged back. */
const KEEP = 60;

export type Bounds = Required<ControlPanelBounds>;

/** Saved bounds, or Java's default place, kept inside the window. */
export function placed(bounds: ControlPanelBounds | null, view = { width: window.innerWidth, height: window.innerHeight }): Bounds {
  const width = Math.max(MIN.width, Math.min(bounds?.width ?? DEFAULT.width, view.width));
  const height = Math.max(MIN.height, Math.min(bounds?.height ?? DEFAULT.height, view.height));
  const x = bounds?.x ?? Math.round((view.width - width) / 2);
  const y = bounds?.y ?? Math.round((view.height - height) / 2);
  return {
    x: Math.round(Math.max(KEEP - width, Math.min(x, view.width - KEEP))),
    y: Math.round(Math.max(0, Math.min(y, view.height - KEEP))),
    width: Math.round(width),
    height: Math.round(height),
  };
}
