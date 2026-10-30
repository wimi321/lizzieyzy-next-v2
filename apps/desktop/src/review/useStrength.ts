import { useCallback } from "react";
import type { Backend, StrengthEstimateView } from "@/api/backend";
import { type ReviewSignals, useLiveQuery } from "./useLiveQuery";

/** The strength estimate for the displayed line, refreshed as analysis arrives. */
export function useStrength(backend: Pick<Backend, "strengthEstimate">, signals: ReviewSignals): StrengthEstimateView | null {
  const read = useCallback(() => backend.strengthEstimate(), [backend]);
  return useLiveQuery(read, signals);
}
