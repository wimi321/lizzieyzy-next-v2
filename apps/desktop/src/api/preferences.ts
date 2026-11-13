import { invoke } from "@tauri-apps/api/core";
import type { UiLanguage } from "./generated/UiLanguage";

export type { UiLanguage };

/** The language and the automatic update check, saved in the settings. */
export interface PreferenceCommands {
  setLanguage(language: UiLanguage): Promise<UiLanguage>;
  setAutoCheckUpdates(enabled: boolean): Promise<boolean>;
}

export const setLanguage: PreferenceCommands["setLanguage"] = (language) =>
  invoke("set_language", { language });
export const setAutoCheckUpdates: PreferenceCommands["setAutoCheckUpdates"] = (enabled) =>
  invoke("set_auto_check_updates", { enabled });
