import { invoke } from "@tauri-apps/api/core";
import type { MoveView } from "./generated/MoveView";
import type { PointView } from "./generated/PointView";
import type { Revision } from "./generated/Revision";
import type { StoneOriginView } from "./generated/StoneOriginView";

export type { StoneOriginView };

export interface MoveEditingCommands {
  /** Uses the supplied color while still enforcing move legality. */
  playColored(mv: MoveView): Promise<Revision>;
  insertMove(mv: MoveView): Promise<Revision>;
  switchStone(point: PointView): Promise<Revision>;
  deleteStone(point: PointView): Promise<Revision>;
  moveStone(from: PointView, to: PointView): Promise<Revision>;
  addVariation(moves: MoveView[]): Promise<Revision>;
  /** Read-only: compare revision with the current document before navigating. */
  stoneOrigin(point: PointView): Promise<StoneOriginView | null>;
  startTrial(): Promise<Revision>;
  endTrial(): Promise<Revision>;
}

export const playColored: MoveEditingCommands["playColored"] = (mv) =>
  invoke("play_colored", { mv });
export const insertMove: MoveEditingCommands["insertMove"] = (mv) =>
  invoke("insert_move", { mv });
export const switchStone: MoveEditingCommands["switchStone"] = (point) =>
  invoke("switch_stone", { point });
export const deleteStone: MoveEditingCommands["deleteStone"] = (point) =>
  invoke("delete_stone", { point });
export const moveStone: MoveEditingCommands["moveStone"] = (from, to) =>
  invoke("move_stone", { from, to });
export const addVariation: MoveEditingCommands["addVariation"] = (moves) =>
  invoke("add_variation", { moves });
export const stoneOrigin: MoveEditingCommands["stoneOrigin"] = (point) =>
  invoke("stone_origin", { point });
export const startTrial: MoveEditingCommands["startTrial"] = () => invoke("start_trial");
export const endTrial: MoveEditingCommands["endTrial"] = () => invoke("end_trial");
