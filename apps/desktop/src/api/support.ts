import { invoke } from "@tauri-apps/api/core";
import type { FullLogStatus } from "./generated/FullLogStatus";

export type { FullLogStatus };

/** Optional until the diagnostics UI is connected. The returned zip path is
 * produced by Rust; no log or settings content is assembled in the browser. */
export interface SupportCommands {
  fullLogStatus?(): Promise<FullLogStatus>;
  startFullLog?(): Promise<FullLogStatus>;
  stopFullLog?(): Promise<FullLogStatus>;
  exportDiagnostics?(): Promise<string>;
  logDirectory?(): Promise<string>;
  revealInFolder?(path: string): Promise<void>;
}

export const fullLogStatus: NonNullable<SupportCommands["fullLogStatus"]> = () => invoke("full_log_status");
export const startFullLog: NonNullable<SupportCommands["startFullLog"]> = () => invoke("start_full_log");
export const stopFullLog: NonNullable<SupportCommands["stopFullLog"]> = () => invoke("stop_full_log");
export const exportDiagnostics: NonNullable<SupportCommands["exportDiagnostics"]> = () => invoke("export_diagnostics");
export const logDirectory: NonNullable<SupportCommands["logDirectory"]> = () => invoke("log_directory");
export const revealInFolder: NonNullable<SupportCommands["revealInFolder"]> = (path) => invoke("reveal_in_folder", { path });
