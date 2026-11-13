import { invoke } from "@tauri-apps/api/core";
import type { Revision } from "./generated/Revision";
import type { BoardTransform } from "./generated/BoardTransform";

export type { BoardTransform };

/** Daily file and native clipboard operations; all rules remain in AppCore. */
export interface EverydayCommands {
  copySgf(): Promise<void>;
  pasteSgf(): Promise<Revision>;
  saveCleanSgf(path: string, keepComments: boolean, currentBranch: boolean): Promise<void>;
  savePng(path: string, bytes: Uint8Array): Promise<void>;
  copyPng(bytes: Uint8Array): Promise<void>;
  transformBoard(transform: BoardTransform): Promise<Revision>;
  alwaysOnTop(): Promise<boolean>;
  setAlwaysOnTop(enabled: boolean): Promise<boolean>;
  uiScale(): Promise<number>;
  setUiScale(scale: number): Promise<number>;
  playSound(): Promise<boolean>;
  setPlaySound(enabled: boolean): Promise<boolean>;
}

export const copySgf: EverydayCommands["copySgf"] = () => invoke("copy_sgf");
export const pasteSgf: EverydayCommands["pasteSgf"] = () => invoke("paste_sgf");
export const saveCleanSgf: EverydayCommands["saveCleanSgf"] = (path, keepComments, currentBranch) =>
  invoke("save_clean_sgf", { path, keepComments, currentBranch });
export const savePng: EverydayCommands["savePng"] = (path, bytes) =>
  invoke("save_png", bytes, { headers: { "x-lizzieyzy-path": encodeURIComponent(path) } });
export const copyPng: EverydayCommands["copyPng"] = (bytes) => invoke("copy_png", bytes);
export const transformBoard: EverydayCommands["transformBoard"] = (transform) =>
  invoke("transform_board", { transform });
export const alwaysOnTop: EverydayCommands["alwaysOnTop"] = () => invoke("always_on_top");
export const setAlwaysOnTop: EverydayCommands["setAlwaysOnTop"] = (enabled) =>
  invoke("set_always_on_top", { enabled });
export const uiScale: EverydayCommands["uiScale"] = () => invoke("ui_scale");
export const setUiScale: EverydayCommands["setUiScale"] = (scale) => invoke("set_ui_scale", { scale });
export const playSound: EverydayCommands["playSound"] = () => invoke("play_sound");
export const setPlaySound: EverydayCommands["setPlaySound"] = (enabled) =>
  invoke("set_play_sound", { enabled });
