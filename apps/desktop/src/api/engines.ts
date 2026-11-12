import { invoke } from "@tauri-apps/api/core";
import type { EngineProfile } from "./generated/EngineProfile";
import type { EngineSettings } from "./generated/EngineSettings";
import type { JavaImportPreview } from "./generated/JavaImportPreview";
import type { Settings } from "./generated/Settings";

export type { EngineEntry } from "./generated/EngineEntry";
export type { EngineRules } from "./generated/EngineRules";
export type { EngineCustomRules } from "./generated/EngineCustomRules";
export type { JavaImportedEngine } from "./generated/JavaImportedEngine";
export type { EngineSettings, JavaImportPreview };

/** Optional until the engine/settings UI moves off configureEngine. */
export interface EngineCommands {
  addEngine?(name: string, profile: EngineProfile): Promise<Settings>;
  updateEngine?(id: string, name: string, profile: EngineProfile): Promise<Settings>;
  removeEngine?(id: string): Promise<Settings>;
  reorderEngines?(ids: string[]): Promise<Settings>;
  selectMainEngine?(id: string): Promise<Settings>;
  restartEngine?(): Promise<Settings>;
  closeEngine?(): Promise<Settings>;
  setEngineSettings?(settings: EngineSettings): Promise<EngineSettings>;
  previewJavaImport?(directory: string): Promise<JavaImportPreview>;
  confirmJavaImport?(previewId: string): Promise<Settings>;
}

export const addEngine: NonNullable<EngineCommands["addEngine"]> = (name, profile) =>
  invoke("add_engine", { name, profile });
export const updateEngine: NonNullable<EngineCommands["updateEngine"]> = (id, name, profile) =>
  invoke("update_engine", { id, name, profile });
export const removeEngine: NonNullable<EngineCommands["removeEngine"]> = (id) =>
  invoke("remove_engine", { id });
export const reorderEngines: NonNullable<EngineCommands["reorderEngines"]> = (ids) =>
  invoke("reorder_engines", { ids });
export const selectMainEngine: NonNullable<EngineCommands["selectMainEngine"]> = (id) =>
  invoke("select_main_engine", { id });
export const restartEngine: NonNullable<EngineCommands["restartEngine"]> = () =>
  invoke("restart_engine");
export const closeEngine: NonNullable<EngineCommands["closeEngine"]> = () =>
  invoke("close_engine");
export const setEngineSettings: NonNullable<EngineCommands["setEngineSettings"]> = (settings) =>
  invoke("set_engine_settings", { settings });
export const previewJavaImport: NonNullable<EngineCommands["previewJavaImport"]> = (directory) =>
  invoke("preview_java_import", { directory });
export const confirmJavaImport: NonNullable<EngineCommands["confirmJavaImport"]> = (previewId) =>
  invoke("confirm_java_import", { previewId });
