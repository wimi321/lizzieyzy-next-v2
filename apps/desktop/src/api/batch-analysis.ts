import { invoke } from "@tauri-apps/api/core";
import type { BatchAnalysisStatus } from "./generated/BatchAnalysisStatus";
import type { BatchMove } from "./generated/BatchMove";
import type { BatchOptions } from "./generated/BatchOptions";

export type { BatchAnalysisStatus, BatchMove, BatchOptions };
export type { BatchOutput } from "./generated/BatchOutput";
export type { BatchPhase } from "./generated/BatchPhase";
export type { BatchItem } from "./generated/BatchItem";
export type { BatchItemState } from "./generated/BatchItemState";
export type { BatchError } from "./generated/BatchError";
export type BatchAnalysisProgressListener = (status: BatchAnalysisStatus) => void;

/** Queue order, state transitions and file naming belong to the Rust actor. */
export interface BatchAnalysisCommands {
  startBatchAnalysis?(options: BatchOptions): Promise<BatchAnalysisStatus>;
  pauseBatchAnalysis?(): Promise<BatchAnalysisStatus>;
  resumeBatchAnalysis?(): Promise<BatchAnalysisStatus>;
  stopBatchAnalysis?(): Promise<BatchAnalysisStatus>;
  addBatchAnalysisFiles?(files: string[]): Promise<BatchAnalysisStatus>;
  removeBatchAnalysisItem?(id: number): Promise<BatchAnalysisStatus>;
  moveBatchAnalysisItem?(id: number, direction: BatchMove): Promise<BatchAnalysisStatus>;
  clearBatchAnalysis?(): Promise<BatchAnalysisStatus>;
  batchAnalysisStatus?(): Promise<BatchAnalysisStatus>;
}

export const startBatchAnalysis: NonNullable<BatchAnalysisCommands["startBatchAnalysis"]> =
  (options) => invoke("start_batch_analysis", { options });
export const pauseBatchAnalysis: NonNullable<BatchAnalysisCommands["pauseBatchAnalysis"]> =
  () => invoke("pause_batch_analysis");
export const resumeBatchAnalysis: NonNullable<BatchAnalysisCommands["resumeBatchAnalysis"]> =
  () => invoke("resume_batch_analysis");
export const stopBatchAnalysis: NonNullable<BatchAnalysisCommands["stopBatchAnalysis"]> =
  () => invoke("stop_batch_analysis");
export const addBatchAnalysisFiles: NonNullable<BatchAnalysisCommands["addBatchAnalysisFiles"]> =
  (files) => invoke("add_batch_analysis_files", { files });
export const removeBatchAnalysisItem: NonNullable<BatchAnalysisCommands["removeBatchAnalysisItem"]> =
  (id) => invoke("remove_batch_analysis_item", { id });
export const moveBatchAnalysisItem: NonNullable<BatchAnalysisCommands["moveBatchAnalysisItem"]> =
  (id, direction) => invoke("move_batch_analysis_item", { id, direction });
export const clearBatchAnalysis: NonNullable<BatchAnalysisCommands["clearBatchAnalysis"]> =
  () => invoke("clear_batch_analysis");
export const batchAnalysisStatus: NonNullable<BatchAnalysisCommands["batchAnalysisStatus"]> =
  () => invoke("batch_analysis_status");
