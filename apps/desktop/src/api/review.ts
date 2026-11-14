import { invoke } from "@tauri-apps/api/core";
import type { FlashAnalysisSettings } from "./generated/FlashAnalysisSettings";
import type { AnalysisView } from "./generated/AnalysisView";
import type { WholeGameOptions } from "./generated/WholeGameOptions";
import type { WholeGameProgress } from "./generated/WholeGameProgress";

export type { WholeGameOptions, WholeGameProgress, FlashAnalysisSettings };
export type { WholeGameScope } from "./generated/WholeGameScope";
export type { WholeGamePreset } from "./generated/WholeGamePreset";
export type { WholeGameStatus } from "./generated/WholeGameStatus";
export type { WholeGameSource } from "./generated/WholeGameSource";
export type WholeGameProgressListener = (progress: WholeGameProgress) => void;

/** All decisions, including option validation and resuming, belong to Rust. */
export interface ReviewCommands {
  flashAnalysis(): Promise<FlashAnalysisSettings>;
  setFlashAnalysis(settings: FlashAnalysisSettings): Promise<FlashAnalysisSettings>;
  startWholeGameAnalysis(options?: WholeGameOptions): Promise<WholeGameProgress>;
  pauseWholeGameAnalysis(): Promise<WholeGameProgress>;
  resumeWholeGameAnalysis(): Promise<WholeGameProgress>;
  cancelWholeGameAnalysis(): Promise<WholeGameProgress>;
  wholeGameProgress(): Promise<WholeGameProgress | null>;
  /** Full root evaluation, candidates and PV for a node in the current document. */
  wholeGameResult(nodeId: number): Promise<AnalysisView | null>;
}

export const startWholeGameAnalysis: ReviewCommands["startWholeGameAnalysis"] =
  (options = { visits: 1000 }) => invoke("start_whole_game_analysis", { options });
export const pauseWholeGameAnalysis: ReviewCommands["pauseWholeGameAnalysis"] =
  () => invoke("pause_whole_game_analysis");
export const resumeWholeGameAnalysis: ReviewCommands["resumeWholeGameAnalysis"] =
  () => invoke("resume_whole_game_analysis");
export const cancelWholeGameAnalysis: ReviewCommands["cancelWholeGameAnalysis"] =
  () => invoke("cancel_whole_game_analysis");
export const wholeGameProgress: ReviewCommands["wholeGameProgress"] =
  () => invoke("whole_game_progress");
export const wholeGameResult: ReviewCommands["wholeGameResult"] =
  (nodeId) => invoke("whole_game_result", { nodeId });

export const flashAnalysis: ReviewCommands["flashAnalysis"] =
  () => invoke("flash_analysis");
export const setFlashAnalysis: ReviewCommands["setFlashAnalysis"] =
  (settings) => invoke("set_flash_analysis", { settings });
