import { invoke } from "@tauri-apps/api/core";
import type { Revision } from "./generated/Revision";

export interface AnalysisCacheCommands {
  cacheEnabled(): Promise<boolean>;
  setCacheEnabled(enabled: boolean): Promise<boolean>;
  /** False clears this position for every engine; true clears the database. */
  clearCache(all: boolean): Promise<void>;
  /** Undoable edit: false clears this node; true clears the whole document. */
  clearAnalysis(all: boolean): Promise<Revision>;
}

export const cacheEnabled: AnalysisCacheCommands["cacheEnabled"] = () =>
  invoke("cache_enabled");
export const setCacheEnabled: AnalysisCacheCommands["setCacheEnabled"] = (enabled) =>
  invoke("set_cache_enabled", { enabled });
export const clearCache: AnalysisCacheCommands["clearCache"] = (all) =>
  invoke("clear_cache", { all });
export const clearAnalysis: AnalysisCacheCommands["clearAnalysis"] = (all) =>
  invoke("clear_analysis", { all });
