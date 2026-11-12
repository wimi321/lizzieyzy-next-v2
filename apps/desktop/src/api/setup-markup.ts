import { invoke } from "@tauri-apps/api/core";
import type { ColorView } from "./generated/ColorView";
import type { MarkupKindView } from "./generated/MarkupKindView";
import type { PointView } from "./generated/PointView";
import type { Revision } from "./generated/Revision";

export type { MarkupKindView };
export type { MarkupView } from "./generated/MarkupView";

export interface SetupMarkupCommands {
  /** Null erases a setup stone. */
  setSetupStone(point: PointView, color: ColorView | null): Promise<Revision>;
  clearSetup(): Promise<Revision>;
  setFirstPlayer(color: ColorView): Promise<Revision>;
  /** Replaces the document with the current board as a new starting position. */
  convertToStartPosition(): Promise<Revision>;
  setMarkup(point: PointView, kind: MarkupKindView): Promise<Revision>;
  clearMarkup(): Promise<Revision>;
  toggleToPlay(): Promise<Revision>;
}

export const setSetupStone: SetupMarkupCommands["setSetupStone"] = (point, color) =>
  invoke("set_setup_stone", { point, color });
export const clearSetup: SetupMarkupCommands["clearSetup"] = () => invoke("clear_setup");
export const setFirstPlayer: SetupMarkupCommands["setFirstPlayer"] = (color) =>
  invoke("set_first_player", { color });
export const convertToStartPosition: SetupMarkupCommands["convertToStartPosition"] = () =>
  invoke("convert_to_start_position");
export const setMarkup: SetupMarkupCommands["setMarkup"] = (point, kind) =>
  invoke("set_markup", { point, kind });
export const clearMarkup: SetupMarkupCommands["clearMarkup"] = () => invoke("clear_markup");
export const toggleToPlay: SetupMarkupCommands["toggleToPlay"] = () => invoke("toggle_to_play");
