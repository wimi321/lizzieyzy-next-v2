import { invoke } from "@tauri-apps/api/core";
import type { DisplaySettings as DisplaySettingsPayload } from "./generated/DisplaySettings";

export type { DisplaySettingsPayload };
// P3-26 will add these defaults to the interface's Required<DisplaySettings> table.
export type DisplaySettings = DisplaySettingsPayload | Omit<DisplaySettingsPayload,
  "autoplay_interval_ms" | "painting_color" | "control_panel_bounds">;
export type { PaintingColor } from "./generated/PaintingColor";
export type { ControlPanelBounds } from "./generated/ControlPanelBounds";
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
