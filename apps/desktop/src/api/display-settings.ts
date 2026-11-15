import { invoke } from "@tauri-apps/api/core";
import type { DisplaySettings as DisplaySettingsPayload } from "./generated/DisplaySettings";

export type { DisplaySettingsPayload };
/** P3-28 will add these defaults to the UI. Keep existing Required tables valid
 * until then; command payloads and Settings.display still use the full DTO. */
type LayoutSettingKey = "layout_mode" | "large_sub_board" | "large_winrate_graph" | "board_position"
  | "floating_boards" | "custom_layouts" | "main_window" | "floating_main" | "floating_sub";
export type DisplaySettings = DisplaySettingsPayload | Omit<DisplaySettingsPayload, LayoutSettingKey>;
export type { PaintingColor } from "./generated/PaintingColor";
export type { WindowBounds as ControlPanelBounds } from "./generated/WindowBounds";
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
