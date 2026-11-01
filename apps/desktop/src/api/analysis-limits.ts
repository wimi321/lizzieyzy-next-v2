import { invoke } from "@tauri-apps/api/core";
import type { AnalysisLimitSettings } from "./generated/AnalysisLimitSettings";
import type { AnalysisState } from "./generated/AnalysisState";

export type { AnalysisLimitSettings, AnalysisState };
export type { AnalysisPauseReason } from "./generated/AnalysisPauseReason";
export type AnalysisStateListener = (state: AnalysisState) => void;

export interface AnalysisLimitCommands {
  analysisLimits(): Promise<AnalysisLimitSettings>;
  setAnalysisLimits(settings: AnalysisLimitSettings): Promise<AnalysisLimitSettings>;
  /** Effective live-analysis intent and pause reason, independent of saved pondering. */
  analysisState(): Promise<AnalysisState>;
  /** Use for Space: resumes a limit pause without turning off saved pondering. */
  togglePondering(): Promise<AnalysisState>;
}

export const analysisLimits: AnalysisLimitCommands["analysisLimits"] = () =>
  invoke("analysis_limits");
export const setAnalysisLimits: AnalysisLimitCommands["setAnalysisLimits"] = (settings) =>
  invoke("set_analysis_limits", { settings });
export const analysisState: AnalysisLimitCommands["analysisState"] = () =>
  invoke("analysis_state");
export const togglePondering: AnalysisLimitCommands["togglePondering"] = () =>
  invoke("toggle_pondering");
