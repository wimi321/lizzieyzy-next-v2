import { invoke } from "@tauri-apps/api/core";
import type { AnalysisExtras } from "./generated/AnalysisExtras";

export type { AnalysisExtras };
export interface AnalysisExtrasCommands {
  setAnalysisExtras?(extras: AnalysisExtras): Promise<void>;
  setPureNetwork?(enabled: boolean): Promise<void>;
}

export const setAnalysisExtras: NonNullable<AnalysisExtrasCommands["setAnalysisExtras"]> = (extras) =>
  invoke("set_analysis_extras", { extras });
export const setPureNetwork: NonNullable<AnalysisExtrasCommands["setPureNetwork"]> = (enabled) =>
  invoke("set_pure_network", { enabled });
