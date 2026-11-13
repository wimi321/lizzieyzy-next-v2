import { invoke } from "@tauri-apps/api/core";
import type { FullLogStatus } from "./generated/FullLogStatus";

export type { FullLogStatus };

/** The returned zip path is produced by Rust; no log or settings content is assembled in the browser. */
export interface SupportCommands {
  fullLogStatus(): Promise<FullLogStatus>;
  startFullLog(): Promise<FullLogStatus>;
  stopFullLog(): Promise<FullLogStatus>;
  exportDiagnostics(): Promise<string>;
  logDirectory(): Promise<string>;
  revealInFolder(path: string): Promise<void>;
}

export const fullLogStatus: SupportCommands["fullLogStatus"] = () => invoke("full_log_status");
export const startFullLog: SupportCommands["startFullLog"] = () => invoke("start_full_log");
export const stopFullLog: SupportCommands["stopFullLog"] = () => invoke("stop_full_log");
export const exportDiagnostics: SupportCommands["exportDiagnostics"] = () => invoke("export_diagnostics");
export const logDirectory: SupportCommands["logDirectory"] = () => invoke("log_directory");
export const revealInFolder: SupportCommands["revealInFolder"] = (path) => invoke("reveal_in_folder", { path });
