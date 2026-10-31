import { useCallback } from "react";
import type { Backend, StrengthEstimateView } from "@/api/backend";
import { type ReviewSignals, useLiveQuery } from "./useLiveQuery";

/**
 * The strength estimate for the displayed line, refreshed as analysis arrives. `bounds` are the
 * phase boundaries the user dragged to; null asks Rust for its defaults.
 */
export function useStrength(
  backend: Pick<Backend, "strengthEstimate">,
  signals: ReviewSignals,
  bounds: readonly [number, number] | null = null,
): StrengthEstimateView | null {
  const first = bounds?.[0];
  const second = bounds?.[1];
  const read = useCallback(
    () => backend.strengthEstimate(first === undefined || second === undefined ? undefined : [first, second]),
    [backend, first, second],
  );
  return useLiveQuery(read, signals);
}
