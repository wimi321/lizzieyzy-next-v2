import type { AccelerationBackend, AccelerationOption, AccelerationPlan, EngineBackend } from "@/api/backend";

/** 显卡提速 texts (`messages.acceleration`). */
export interface AccelerationLabels {
  intro: string;
  gpus: string;
  noGpu: string;
  driver: string;
  current: string;
  backends: Record<EngineBackend, string>;
  describe: Record<AccelerationBackend, string>;
  recommended: string;
  inUse: string;
  installed: string;
  download: string;
  oldDriver: string;
  unsupported: Record<AccelerationBackend, string>;
  reasons: Record<string, string>;
  loading: string;
  enable: string;
  disable: string;
  downloading: string;
  installing: string;
  paused: string;
  pause: string;
  resume: string;
  cancel: string;
  starting: string;
  tuningOpenCL: string;
  tuningTensorRT: string;
  succeeded: string;
  benchmark: string;
  failed: string;
  timedOut: string;
  cancelled: string;
  restored: string;
  output: string;
  copy: string;
  copied: string;
  hint: string;
  hintOpen: string;
  hintDismiss: string;
  close: string;
  busy: string;
  noEngine: string;
}

const GPU_BACKENDS: EngineBackend[] = ["cuda", "tensorrt", "opencl"];

/** Options Rust offered, in its order; entries without a backend are ignored. */
export function optionsOf(plan: AccelerationPlan | null): (AccelerationOption & { backend: AccelerationBackend })[] {
  return (plan?.options ?? []).filter((option): option is AccelerationOption & { backend: AccelerationBackend } => option.backend !== undefined);
}

export function onGpu(plan: AccelerationPlan | null): boolean {
  return plan?.current !== undefined && GPU_BACKENDS.includes(plan.current);
}

/** The first recommended option not in use, else the first other supported one, else none. */
export function defaultChoice(plan: AccelerationPlan | null): AccelerationBackend | null {
  const options = optionsOf(plan).filter((option) => option.supported && option.backend !== plan?.current);
  return (options.find((option) => option.recommended) ?? options[0])?.backend ?? null;
}

/**
 * The GPU named in the one-time hint: only on a machine with options, while the bundled CPU
 * engine runs and something is recommended. The adapter that matches the recommendation leads.
 */
export function hintGpu(plan: AccelerationPlan | null): string | null {
  if (!plan || plan.current !== "eigen") return null;
  const recommended = optionsOf(plan).find((option) => option.recommended && option.supported);
  if (!recommended) return null;
  const gpus = (plan.gpus ?? []).filter((gpu) => gpu.name);
  const nvidia = recommended.backend !== "opencl";
  return (gpus.find((gpu) => Boolean(gpu.nvidia) === nvidia) ?? gpus[0])?.name ?? null;
}

export function size(bytes: number | undefined): string {
  const value = bytes ?? 0;
  return value >= 1_073_741_824 ? `${(value / 1_073_741_824).toFixed(1)} GB` : `${(value / 1_048_576).toFixed(1)} MB`;
}

export const mb = (bytes: number | undefined) => ((bytes ?? 0) / 1_048_576).toFixed(1);

export type Failure =
  | { kind: "Failed"; reason: string; tail: string[] }
  | { kind: "TimedOut"; tail: string[] }
  | { kind: "Cancelled"; tail: string[] }
  | { kind: "Busy" | "NoEngine" | "Unsupported" }
  | { kind: "Other"; reason: string };

/** Reads a rejected acceleration command: `{ type: "Acceleration", data: AccelerationError }`. */
export function failureOf(error: unknown): Failure {
  const outer = error as { type?: unknown; data?: unknown } | null;
  if (outer && typeof outer === "object" && outer.type === "Acceleration" && outer.data && typeof outer.data === "object") {
    const inner = outer.data as { type?: unknown; data?: { reason?: unknown; stderr_tail?: unknown } };
    const tail = Array.isArray(inner.data?.stderr_tail) ? inner.data.stderr_tail.map(String) : [];
    switch (inner.type) {
      case "Failed":
        return { kind: "Failed", reason: String(inner.data?.reason ?? ""), tail };
      case "TimedOut":
        return { kind: "TimedOut", tail };
      case "Cancelled":
        return { kind: "Cancelled", tail };
      case "Busy":
        return { kind: "Busy" };
      case "NoEngine":
        return { kind: "NoEngine" };
      case "UnsupportedPlatform":
      case "UnsupportedBackend":
        return { kind: "Unsupported" };
    }
  }
  if (outer && typeof outer === "object" && typeof outer.data === "string") return { kind: "Other", reason: outer.data };
  if (outer && typeof outer === "object" && outer.data && typeof outer.data === "object" && "reason" in outer.data) {
    return { kind: "Other", reason: String((outer.data as { reason: unknown }).reason) };
  }
  return { kind: "Other", reason: error instanceof Error ? error.message : String(error) };
}
