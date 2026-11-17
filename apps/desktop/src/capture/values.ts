import type { CaptureSettings } from "@/api/backend";

/** Java's defaults (`Config` capture-*). */
export const CAPTURE_DEFAULTS: Required<CaptureSettings> = { black_offset: 120, black_percent: 25, white_offset: 120, white_percent: 15, gray_offset: 80 };

export type CaptureField = keyof Required<CaptureSettings>;

/** Java's order, and the ranges Java writes in its labels but never checks. */
export const CAPTURE_FIELDS: readonly { key: CaptureField; min: number; max: number }[] = [
  { key: "black_offset", min: 0, max: 255 },
  { key: "white_offset", min: 0, max: 255 },
  { key: "black_percent", min: 1, max: 100 },
  { key: "white_percent", min: 1, max: 100 },
  { key: "gray_offset", min: 0, max: 255 },
];

export const fill = (text: string, values: Record<string, string | number>) => text.replace(/\{(\w+)\}/g, (whole, key: string) => (key in values ? String(values[key]) : whole));

/** Rust's typed errors: the innermost reason, a variant name, or the message. */
export function describe(error: unknown): string {
  let value: unknown = error;
  for (let depth = 0; depth < 4 && value && typeof value === "object"; depth++) {
    const record = value as { data?: unknown; reason?: unknown; type?: unknown };
    if (typeof record.reason === "string") return record.reason;
    if (typeof record.data === "string") return record.data;
    if (record.data && typeof record.data === "object") {
      value = record.data;
      continue;
    }
    if (typeof record.type === "string") return record.type;
  }
  return error instanceof Error ? error.message : String(error);
}

/**
 * Sends only the newest value, at most once per `interval` ms and one at a time; a reply to a
 * value sent before `drop()` is ignored. Java's helper recognizes on every mouse move instead.
 */
export class LatestOnly<T, R> {
  private pending: T | null = null;
  private inFlight = false;
  private last = -Infinity;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private generation = 0;

  constructor(
    private readonly send: (value: T) => Promise<R>,
    private readonly onResult: (result: R | null) => void,
    private readonly interval: number,
    private readonly now: () => number = Date.now,
  ) {}

  push(value: T) {
    this.pending = value;
    this.pump();
  }

  /** Forget what is waiting and ignore what is on its way. */
  drop() {
    this.generation++;
    this.pending = null;
    clearTimeout(this.timer);
  }

  private pump = () => {
    if (this.inFlight || this.pending === null) return;
    const wait = this.last + this.interval - this.now();
    if (wait > 0) {
      clearTimeout(this.timer);
      this.timer = setTimeout(this.pump, wait);
      return;
    }
    const value = this.pending;
    const generation = this.generation;
    this.pending = null;
    this.inFlight = true;
    this.last = this.now();
    this.send(value)
      .then((result) => generation === this.generation && this.onResult(result))
      .catch(() => generation === this.generation && this.onResult(null))
      .finally(() => {
        this.inFlight = false;
        this.pump();
      });
  };
}
