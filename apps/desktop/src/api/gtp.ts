import { invoke } from "@tauri-apps/api/core";
import type { GtpEngineEntry } from "./generated/GtpEngineEntry";
import type { GtpEngineInfo } from "./generated/GtpEngineInfo";
import type { GtpConsoleLine } from "./generated/GtpConsoleLine";
import type { Settings } from "./generated/Settings";
export type { GtpEngineEntry, GtpEngineInfo, GtpConsoleLine, Settings };
export type { GtpTime } from "./generated/GtpTime";
export type GtpConsoleListener = (line: GtpConsoleLine) => void;
export interface GtpCommands {
  addGtpEngine?(entry: GtpEngineEntry): Promise<Settings>;
  updateGtpEngine?(entry: GtpEngineEntry): Promise<Settings>;
  removeGtpEngine?(id: string): Promise<Settings>;
  testGtpEngine?(id: string): Promise<GtpEngineInfo>;
  gtpConsole?(engineId: string): Promise<GtpConsoleLine[]>;
  gtpConsoleSend?(engineId: string, text: string): Promise<string>;
}
export const addGtpEngine: NonNullable<GtpCommands["addGtpEngine"]> = (entry) => invoke("add_gtp_engine", { entry });
export const updateGtpEngine: NonNullable<GtpCommands["updateGtpEngine"]> = (entry) => invoke("update_gtp_engine", { entry });
export const removeGtpEngine: NonNullable<GtpCommands["removeGtpEngine"]> = (id) => invoke("remove_gtp_engine", { id });
export const testGtpEngine: NonNullable<GtpCommands["testGtpEngine"]> = (id) => invoke("test_gtp_engine", { id });
export const gtpConsole: NonNullable<GtpCommands["gtpConsole"]> = (engineId) => invoke("gtp_console", { engineId });
export const gtpConsoleSend: NonNullable<GtpCommands["gtpConsoleSend"]> = (engineId, text) => invoke("gtp_console_send", { engineId, text });
