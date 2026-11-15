import { invoke } from "@tauri-apps/api/core";
import type { SaveSlot } from "./generated/SaveSlot";
import type { Revision } from "./generated/Revision";

export type { SaveSlot } from "./generated/SaveSlot";
export type { SlotBoard } from "./generated/SlotBoard";
export type { SaveSlotsError } from "./generated/SaveSlotsError";

export interface SaveSlotCommands {
  saveSlots?(): Promise<SaveSlot[]>;
  saveToNewSlot?(name: string): Promise<SaveSlot>;
  overwriteSlot?(id: string): Promise<SaveSlot>;
  renameSlot?(id: string, name: string): Promise<SaveSlot>;
  loadSlot?(id: string): Promise<Revision>;
  deleteSlot?(id: string): Promise<void>;
  deleteAllSlots?(): Promise<void>;
  setAutosave?(enabled: boolean): Promise<boolean>;
  setRestoreLastGame?(enabled: boolean): Promise<boolean>;
}

export const saveSlots: NonNullable<SaveSlotCommands["saveSlots"]> = () =>
  invoke("save_slots");
export const saveToNewSlot: NonNullable<SaveSlotCommands["saveToNewSlot"]> = (name) =>
  invoke("save_to_new_slot", { name });
export const overwriteSlot: NonNullable<SaveSlotCommands["overwriteSlot"]> = (id) =>
  invoke("overwrite_slot", { id });
export const renameSlot: NonNullable<SaveSlotCommands["renameSlot"]> = (id, name) =>
  invoke("rename_slot", { id, name });
export const loadSlot: NonNullable<SaveSlotCommands["loadSlot"]> = (id) =>
  invoke("load_slot", { id });
export const deleteSlot: NonNullable<SaveSlotCommands["deleteSlot"]> = (id) =>
  invoke("delete_slot", { id });
export const deleteAllSlots: NonNullable<SaveSlotCommands["deleteAllSlots"]> = () =>
  invoke("delete_all_slots");
export const setAutosave: NonNullable<SaveSlotCommands["setAutosave"]> = (enabled) =>
  invoke("set_autosave", { enabled });
export const setRestoreLastGame: NonNullable<SaveSlotCommands["setRestoreLastGame"]> = (enabled) =>
  invoke("set_restore_last_game", { enabled });
