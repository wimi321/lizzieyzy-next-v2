import { invoke } from "@tauri-apps/api/core";
import type { Revision } from "./generated/Revision";
import type { TencentPage } from "./generated/TencentPage";
import type { TencentQuery } from "./generated/TencentQuery";

export type { TencentPage, TencentQuery };
export type { TencentGame } from "./generated/TencentGame";
export type { TencentPlayer } from "./generated/TencentPlayer";
export type { TencentRank } from "./generated/TencentRank";
export type { TencentFailure } from "./generated/TencentFailure";
export type { OpenUrlFailure } from "./generated/OpenUrlFailure";

/** Rust owns URL routing, downloading, parsing and query history. */
export interface TencentCommands {
  tencentSearch?(query: TencentQuery): Promise<TencentPage>;
  tencentNextPage?(query: TencentQuery, cursor: string): Promise<TencentPage>;
  tencentOpenGame?(chessid: string): Promise<Revision>;
  tencentClearHistory?(): Promise<void>;
  openUrl?(url: string): Promise<Revision>;
}

export const tencentSearch: NonNullable<TencentCommands["tencentSearch"]> = (query) =>
  invoke("tencent_search", { query });
export const tencentNextPage: NonNullable<TencentCommands["tencentNextPage"]> = (query, cursor) =>
  invoke("tencent_next_page", { query, cursor });
export const tencentOpenGame: NonNullable<TencentCommands["tencentOpenGame"]> = (chessid) =>
  invoke("tencent_open_game", { chessid });
export const tencentClearHistory: NonNullable<TencentCommands["tencentClearHistory"]> = () =>
  invoke("tencent_clear_history");
export const openUrl: NonNullable<TencentCommands["openUrl"]> = (url) => invoke("open_url", { url });
