import { invoke } from "@tauri-apps/api/core";
import type { GameInfoEdit } from "./generated/GameInfoEdit";
import type { NewGameOptions } from "./generated/NewGameOptions";
import type { Revision } from "./generated/Revision";

export type { GameInfoEdit, NewGameOptions };

export interface EditCommands {
  /** An empty string removes the current node's comment. */
  setComment(text: string): Promise<Revision>;
  deleteNode(nodeId: number): Promise<Revision>;
  deleteBranch(nodeId: number): Promise<Revision>;
  /** Promotes the complete root-to-node path as one undoable edit. */
  promoteToMain(nodeId: number): Promise<Revision>;
  newGame(options: NewGameOptions): Promise<Revision>;
  setGameInfo(info: GameInfoEdit): Promise<Revision>;
  recentFiles(): Promise<string[]>;
  clearRecentFiles(): Promise<void>;
}

export const setComment: EditCommands["setComment"] = (text) => invoke("set_comment", { text });
export const deleteNode: EditCommands["deleteNode"] = (nodeId) => invoke("delete_node", { nodeId });
export const deleteBranch: EditCommands["deleteBranch"] = (nodeId) => invoke("delete_branch", { nodeId });
export const promoteToMain: EditCommands["promoteToMain"] = (nodeId) => invoke("promote_to_main", { nodeId });
export const newGame: EditCommands["newGame"] = (options) => invoke("new_game", { options });
export const setGameInfo: EditCommands["setGameInfo"] = (info) => invoke("set_game_info", { info });
export const recentFiles: EditCommands["recentFiles"] = () => invoke("recent_files");
export const clearRecentFiles: EditCommands["clearRecentFiles"] = () => invoke("clear_recent_files");
