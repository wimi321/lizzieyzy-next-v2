import { invoke } from "@tauri-apps/api/core";
import type { Revision } from "./generated/Revision";
import type { TsumegoOptions } from "./generated/TsumegoOptions";
import type { TsumegoStatus } from "./generated/TsumegoStatus";

export type { TsumegoOptions, TsumegoStatus };
export type { TsumegoToPlay } from "./generated/TsumegoToPlay";
export type { TsumegoAttacker } from "./generated/TsumegoAttacker";
export type { TsumegoKoThreat } from "./generated/TsumegoKoThreat";

export interface TsumegoCommands {
  /** Generate from the original position, including when replacing an active problem. */
  generateTsumego(options: TsumegoOptions): Promise<Revision>;
  /** Restore the original document and focus; a no-op when no problem is active. */
  exitTsumego(): Promise<Revision>;
  /** Subscribe to DocumentChanged before reading; discard replies from older revisions. */
  tsumegoStatus(): Promise<TsumegoStatus>;
}

export const generateTsumego: TsumegoCommands["generateTsumego"] = (options) =>
  invoke("generate_tsumego", { options });
export const exitTsumego: TsumegoCommands["exitTsumego"] = () => invoke("exit_tsumego");
export const tsumegoStatus: TsumegoCommands["tsumegoStatus"] = () => invoke("tsumego_status");
