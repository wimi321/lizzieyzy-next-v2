import { invoke } from "@tauri-apps/api/core";
import type { DisplaySettings } from "./generated/DisplaySettings";

export type { DisplaySettings };
export type { MoveNumberMode } from "./generated/MoveNumberMode";
export type { MoveRankMarkMode } from "./generated/MoveRankMarkMode";
export type { NextMoveHint } from "./generated/NextMoveHint";
export type { WinrateGraphMode } from "./generated/WinrateGraphMode";

export interface DisplaySettingsCommands {
  /** Replace and persist the display preferences; invalid values leave them unchanged. */
  setDisplaySettings(display: DisplaySettings): Promise<DisplaySettings>;
}

export const setDisplaySettings: DisplaySettingsCommands["setDisplaySettings"] = (display) =>
  invoke("set_display_settings", { display });
