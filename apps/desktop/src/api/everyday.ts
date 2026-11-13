import { invoke } from "@tauri-apps/api/core";
import type { Revision } from "./generated/Revision";
import type { BoardTransform } from "./generated/BoardTransform";

export type { BoardTransform };

/** Daily file and native clipboard operations; all rules remain in AppCore. */
export interface EverydayCommands {
  copySgf?(): Promise<void>;
  pasteSgf?(): Promise<Revision>;
  saveCleanSgf?(path: string, keepComments: boolean, currentBranch: boolean): Promise<void>;
  savePng?(path: string, bytes: Uint8Array): Promise<void>;
  copyPng?(bytes: Uint8Array): Promise<void>;
  transformBoard?(transform: BoardTransform): Promise<Revision>;
  alwaysOnTop?(): Promise<boolean>;
  setAlwaysOnTop?(enabled: boolean): Promise<boolean>;
  uiScale?(): Promise<number>;
  setUiScale?(scale: number): Promise<number>;
  playSound?(): Promise<boolean>;
  setPlaySound?(enabled: boolean): Promise<boolean>;
}

export const copySgf: NonNullable<EverydayCommands["copySgf"]> = () => invoke("copy_sgf");
export const pasteSgf: NonNullable<EverydayCommands["pasteSgf"]> = () => invoke("paste_sgf");
export const saveCleanSgf: NonNullable<EverydayCommands["saveCleanSgf"]> = (path, keepComments, currentBranch) =>
  invoke("save_clean_sgf", { path, keepComments, currentBranch });
export const savePng: NonNullable<EverydayCommands["savePng"]> = (path, bytes) =>
  invoke("save_png", bytes, { headers: { "x-lizzieyzy-path": encodeURIComponent(path) } });
export const copyPng: NonNullable<EverydayCommands["copyPng"]> = (bytes) => invoke("copy_png", bytes);
export const transformBoard: NonNullable<EverydayCommands["transformBoard"]> = (transform) =>
  invoke("transform_board", { transform });
export const alwaysOnTop: NonNullable<EverydayCommands["alwaysOnTop"]> = () => invoke("always_on_top");
export const setAlwaysOnTop: NonNullable<EverydayCommands["setAlwaysOnTop"]> = (enabled) =>
  invoke("set_always_on_top", { enabled });
export const uiScale: NonNullable<EverydayCommands["uiScale"]> = () => invoke("ui_scale");
export const setUiScale: NonNullable<EverydayCommands["setUiScale"]> = (scale) => invoke("set_ui_scale", { scale });
export const playSound: NonNullable<EverydayCommands["playSound"]> = () => invoke("play_sound");
export const setPlaySound: NonNullable<EverydayCommands["setPlaySound"]> = (enabled) =>
  invoke("set_play_sound", { enabled });
