import { invoke } from "@tauri-apps/api/core";
import type { CommentaryKeyStatus } from "./generated/CommentaryKeyStatus";
import type { CommentaryRequest } from "./generated/CommentaryRequest";
import type { CommentarySettings } from "./generated/CommentarySettings";
import type { CommentaryUpdated } from "./generated/CommentaryUpdated";

export type { CommentaryKeyStatus, CommentaryRequest, CommentarySettings, CommentaryUpdated };
export type { CommentaryMode } from "./generated/CommentaryMode";
export type { CommentaryPhase } from "./generated/CommentaryPhase";
export type { CommentaryPosition } from "./generated/CommentaryPosition";
export type { CommentaryError } from "./generated/CommentaryError";
export type { CommentaryStyle } from "./generated/CommentaryStyle";
export type { CommentaryTerminology } from "./generated/CommentaryTerminology";
export type { CommentaryDetail } from "./generated/CommentaryDetail";
export type { CommentaryVariations } from "./generated/CommentaryVariations";
export type CommentaryUpdatedListener = (view: CommentaryUpdated) => void;

/** Rust owns requests, grounding, cancellation and undoable comment edits. */
export interface CommentaryCommands {
  startCommentary?(request: CommentaryRequest): Promise<CommentaryUpdated>;
  stopCommentary?(): Promise<CommentaryUpdated>;
  commentaryStatus?(): Promise<CommentaryUpdated>;
  commentaryModels?(): Promise<string[]>;
  setCommentaryKey?(key: string, remember: boolean): Promise<CommentaryKeyStatus>;
  clearCommentaryKey?(): Promise<CommentaryKeyStatus>;
  commentaryKeyStatus?(): Promise<CommentaryKeyStatus>;
  setCommentarySettings?(settings: CommentarySettings): Promise<CommentarySettings>;
}

export const startCommentary: NonNullable<CommentaryCommands["startCommentary"]> = (request) => invoke("start_commentary", { request });
export const stopCommentary: NonNullable<CommentaryCommands["stopCommentary"]> = () => invoke("stop_commentary");
export const commentaryStatus: NonNullable<CommentaryCommands["commentaryStatus"]> = () => invoke("commentary_status");
export const commentaryModels: NonNullable<CommentaryCommands["commentaryModels"]> = () => invoke("commentary_models");
export const setCommentaryKey: NonNullable<CommentaryCommands["setCommentaryKey"]> = (key, remember) => invoke("set_commentary_key", { key, remember });
export const clearCommentaryKey: NonNullable<CommentaryCommands["clearCommentaryKey"]> = () => invoke("clear_commentary_key");
export const commentaryKeyStatus: NonNullable<CommentaryCommands["commentaryKeyStatus"]> = () => invoke("commentary_key_status");
export const setCommentarySettings: NonNullable<CommentaryCommands["setCommentarySettings"]> = (settings) => invoke("set_commentary_settings", { settings });
