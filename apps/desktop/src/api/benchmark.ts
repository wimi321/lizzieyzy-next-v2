import { invoke } from "@tauri-apps/api/core";
import type { BenchmarkStatus } from "./generated/BenchmarkStatus";

export type { BenchmarkStatus };
export type { BenchmarkPhase } from "./generated/BenchmarkPhase";
export type { BenchmarkProgress } from "./generated/BenchmarkProgress";
export type { BenchmarkMeasurement } from "./generated/BenchmarkMeasurement";
export type { BenchmarkResult } from "./generated/BenchmarkResult";
export type BenchmarkProgressListener = (status: BenchmarkStatus) => void;

export interface BenchmarkCommands {
  /** Manual only. Resolves after launch; completion arrives through progress. */
  startBenchmark(): Promise<BenchmarkStatus>;
  /** Resolves after the benchmark exits and analysis is restored. */
  cancelBenchmark(): Promise<BenchmarkStatus>;
  /** Current run and saved result, including optional official batch/NN-thread advice.
   * Display speeds remain the recommended thread's main measurement, as in Java. */
  benchmarkStatus(): Promise<BenchmarkStatus>;
  /** Clear this engine's optimization and restore its original configuration. */
  resetBenchmark(): Promise<BenchmarkStatus>;
}

export const startBenchmark: BenchmarkCommands["startBenchmark"] = () => invoke("start_benchmark");
export const cancelBenchmark: BenchmarkCommands["cancelBenchmark"] = () => invoke("cancel_benchmark");
export const benchmarkStatus: BenchmarkCommands["benchmarkStatus"] = () => invoke("benchmark_status");
export const resetBenchmark: BenchmarkCommands["resetBenchmark"] = () => invoke("reset_benchmark");
