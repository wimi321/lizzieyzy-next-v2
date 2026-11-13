import { invoke } from "@tauri-apps/api/core";
import type { UiLanguage } from "./generated/UiLanguage";

export type { UiLanguage };

/** Optional until the preferences UI consumes these persisted settings. */
export interface PreferenceCommands {
  setLanguage?(language: UiLanguage): Promise<UiLanguage>;
  setAutoCheckUpdates?(enabled: boolean): Promise<boolean>;
}

export const setLanguage: NonNullable<PreferenceCommands["setLanguage"]> = (language) =>
  invoke("set_language", { language });
export const setAutoCheckUpdates: NonNullable<PreferenceCommands["setAutoCheckUpdates"]> = (enabled) =>
  invoke("set_auto_check_updates", { enabled });
