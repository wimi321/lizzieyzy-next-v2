import { invoke } from "@tauri-apps/api/core";
import type { StrengthEstimateView } from "./generated/StrengthEstimateView";

export type { StrengthEstimateView };
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
   * replies whose revision no longer matches the document. */
  strengthEstimate(): Promise<StrengthEstimateView>;
}

export const strengthEstimate: StrengthCommands["strengthEstimate"] = () =>
  invoke("strength_estimate");
