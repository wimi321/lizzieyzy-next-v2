import { invoke } from "@tauri-apps/api/core";
import type { StrengthEstimateView } from "./generated/StrengthEstimateView";
import type { StrengthPhasesReport } from "./generated/StrengthPhasesReport";

export type { StrengthEstimateView, StrengthPhasesReport };
export type { PhaseEstimate } from "./generated/PhaseEstimate";
export type { PhaseExtremes } from "./generated/PhaseExtremes";
export type { StrengthReport } from "./generated/StrengthReport";
export type { SideReport } from "./generated/SideReport";
export type { PhaseReport } from "./generated/PhaseReport";
export type { Confidence } from "./generated/Confidence";
export type { Sample } from "./generated/Sample";
export type { MoveCategory } from "./generated/MoveCategory";

export interface StrengthCommands {
  /** Read estimates for the current displayed line without navigating or
   * starting analysis. Subscribe before reading, refresh on DocumentChanged,
   * AnalysisUpdated and WholeGameProgress, and discard obsolete requests or
   * replies whose revision no longer matches the document. Bounds are the last
   * opening/middle moves; Rust normalizes them and echoes the used bounds in
   * phases.bounds. Omit them to use the backend defaults. */
  strengthEstimate(bounds?: StrengthPhasesReport["bounds"]): Promise<StrengthEstimateView>;
}

export const strengthEstimate: StrengthCommands["strengthEstimate"] = (bounds) =>
  invoke("strength_estimate", { bounds: bounds ?? null });
