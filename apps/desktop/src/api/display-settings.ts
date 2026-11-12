import { invoke } from "@tauri-apps/api/core";
import type { DisplaySettings as WireDisplaySettings } from "./generated/DisplaySettings";

/** Keep the previous UI's Required<> defaults compatible until its panel card
 * adds show_list_pane; commands and generated DTOs always use the complete shape. */
export type DisplaySettings = WireDisplaySettings | Omit<WireDisplaySettings, "show_list_pane">;
export type DisplaySettingsPayload = WireDisplaySettings;
export type { MoveNumberMode } from "./generated/MoveNumberMode";
export type { MoveRankMarkMode } from "./generated/MoveRankMarkMode";
export type { NextMoveHint } from "./generated/NextMoveHint";
export type { WinrateGraphMode } from "./generated/WinrateGraphMode";

export interface DisplaySettingsCommands {
  /** Replace and persist the display preferences; invalid values leave them unchanged. */
  setDisplaySettings(display: DisplaySettingsPayload): Promise<DisplaySettingsPayload>;
}

export const setDisplaySettings: DisplaySettingsCommands["setDisplaySettings"] = (display) =>
  invoke("set_display_settings", { display });
