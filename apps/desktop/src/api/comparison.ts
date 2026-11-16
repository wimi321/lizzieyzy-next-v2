import { invoke } from "@tauri-apps/api/core";
import type { AnalysisView } from "./generated/AnalysisView";
import type { ComparisonView } from "./generated/ComparisonView";
import type { Settings } from "./generated/Settings";

export type { ComparisonView };
export type ComparisonChangedListener = (view: ComparisonView) => void;
export type ComparisonAnalysisUpdatedListener = (analysis: AnalysisView) => void;

export interface ComparisonCommands {
  comparisonState?(): Promise<ComparisonView>;
  selectComparisonEngine?(id: string | null): Promise<Settings>;
  restartComparisonEngine?(): Promise<Settings>;
  closeComparisonEngine?(): Promise<Settings>;
  closeOtherEngines?(): Promise<Settings>;
  closeAllEngines?(): Promise<Settings>;
}

export const comparisonState: NonNullable<ComparisonCommands["comparisonState"]> = () =>
  invoke("comparison_state");
export const selectComparisonEngine: NonNullable<ComparisonCommands["selectComparisonEngine"]> = (id) =>
  invoke("select_comparison_engine", { id });
export const restartComparisonEngine: NonNullable<ComparisonCommands["restartComparisonEngine"]> = () =>
  invoke("restart_comparison_engine");
export const closeComparisonEngine: NonNullable<ComparisonCommands["closeComparisonEngine"]> = () =>
  invoke("close_comparison_engine");
export const closeOtherEngines: NonNullable<ComparisonCommands["closeOtherEngines"]> = () =>
  invoke("close_other_engines");
export const closeAllEngines: NonNullable<ComparisonCommands["closeAllEngines"]> = () =>
  invoke("close_all_engines");
