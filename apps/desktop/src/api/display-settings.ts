import { invoke } from "@tauri-apps/api/core";
import type { DisplaySettings as DisplaySettingsPayload } from "./generated/DisplaySettings";

export type { DisplaySettingsPayload };
// Compatibility for P3-07: the existing UI uses Required<DisplaySettings> for
// its older defaults. IPC always accepts/returns the complete generated DTO.
export type DisplaySettings = DisplaySettingsPayload | Omit<DisplaySettingsPayload,
  "estimate_location" | "estimate_style" | "pure_estimate_style" | "remember_estimate" | "moves_ownership" | "estimate_shortcut" | "sub_board_heat" | "sub_board_raw" | "top_toolbar_layout" | "top_toolbar_wrap" | "top_toolbar_hidden" | "bottom_toolbar_hidden">;
export type { EstimateLocation } from "./generated/EstimateLocation";
export type { EstimateStyle } from "./generated/EstimateStyle";
export type { SubBoardHeat } from "./generated/SubBoardHeat";
export type { TopToolbarLayout } from "./generated/TopToolbarLayout";
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
