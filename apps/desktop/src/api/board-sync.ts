import { invoke } from "@tauri-apps/api/core";
import type { BoardSyncOptions } from "./generated/BoardSyncOptions";
import type { BoardSyncStatus } from "./generated/BoardSyncStatus";

export type { BoardSyncOptions, BoardSyncStatus };
export type { BoardSyncError } from "./generated/BoardSyncError";
export type { BoardSyncLanguage } from "./generated/BoardSyncLanguage";
export type { BoardSyncPhase } from "./generated/BoardSyncPhase";
export type BoardSyncStatusListener = (status: BoardSyncStatus) => void;

export interface BoardSyncCommands {
  /** Windows only. Resolves after launch; protocol readiness arrives as status. */
  startBoardSync(options?: BoardSyncOptions): Promise<BoardSyncStatus>;
  /** Resolves after the recognizer process has exited. */
  stopBoardSync(): Promise<BoardSyncStatus>;
  boardSyncStatus(): Promise<BoardSyncStatus>;
}

export const startBoardSync: BoardSyncCommands["startBoardSync"] =
  (options = { always_sync: true, always_goto_last: false, language: "Cn" }) => invoke("start_board_sync", { options });
export const stopBoardSync: BoardSyncCommands["stopBoardSync"] = () => invoke("stop_board_sync");
export const boardSyncStatus: BoardSyncCommands["boardSyncStatus"] = () => invoke("board_sync_status");
