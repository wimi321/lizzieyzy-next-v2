import { invoke } from "@tauri-apps/api/core";
import type { WebBoardOptions } from "./generated/WebBoardOptions";
import type { WebBoardStatus } from "./generated/WebBoardStatus";

export type { WebBoardOptions, WebBoardStatus };
export type { WebBoardError } from "./generated/WebBoardError";
export type { WebServerMessage } from "./generated/WebServerMessage";
export type { WebClientMessage } from "./generated/WebClientMessage";
export type WebBoardChangedListener = (status: WebBoardStatus) => void;

export interface WebBoardCommands {
  startWebBoard?(): Promise<WebBoardStatus>;
  stopWebBoard?(): Promise<void>;
  webBoardStatus?(): Promise<WebBoardStatus>;
  setWebBoardOptions?(options: WebBoardOptions): Promise<void>;
  forceEndWebTrial?(): Promise<void>;
}

export const startWebBoard: NonNullable<WebBoardCommands["startWebBoard"]> = () =>
  invoke("start_web_board");
export const stopWebBoard: NonNullable<WebBoardCommands["stopWebBoard"]> = () =>
  invoke("stop_web_board");
export const webBoardStatus: NonNullable<WebBoardCommands["webBoardStatus"]> = () =>
  invoke("web_board_status");
export const setWebBoardOptions: NonNullable<WebBoardCommands["setWebBoardOptions"]> = (options) =>
  invoke("set_web_board_options", { options });
export const forceEndWebTrial: NonNullable<WebBoardCommands["forceEndWebTrial"]> = () =>
  invoke("force_end_web_trial");
