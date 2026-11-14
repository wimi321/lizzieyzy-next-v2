import { invoke } from "@tauri-apps/api/core";
import type { MatchSetup } from "./generated/MatchSetup";
import type { MatchView } from "./generated/MatchView";
import type { MatchSide } from "./generated/MatchSide";
import type { ColorView } from "./generated/ColorView";
export type { MatchSetup, MatchView, MatchSide, ColorView };
export type { MatchPlayer } from "./generated/MatchPlayer";
export type MatchUpdatedListener = (view: MatchView) => void;
export interface MatchCommands {
  startMatch?(setup: MatchSetup): Promise<MatchView>;
  pauseMatch?(): Promise<MatchView>;
  resumeMatch?(): Promise<MatchView>;
  stopMatch?(): Promise<MatchView>;
  setMatchGames?(n: number): Promise<MatchView>;
  matchPlayNow?(): Promise<MatchView>;
  matchResign?(color: ColorView): Promise<MatchView>;
  setMatchSideParams?(side: MatchSide, wrn: number, pda: number): Promise<MatchView>;
  matchView?(): Promise<MatchView | null>;
}
export const startMatch: NonNullable<MatchCommands["startMatch"]> = (setup) => invoke("start_match", { setup });
export const pauseMatch: NonNullable<MatchCommands["pauseMatch"]> = () => invoke("pause_match");
export const resumeMatch: NonNullable<MatchCommands["resumeMatch"]> = () => invoke("resume_match");
export const stopMatch: NonNullable<MatchCommands["stopMatch"]> = () => invoke("stop_match");
export const setMatchGames: NonNullable<MatchCommands["setMatchGames"]> = (n) => invoke("set_match_games", { n });
export const matchPlayNow: NonNullable<MatchCommands["matchPlayNow"]> = () => invoke("match_play_now");
export const matchResign: NonNullable<MatchCommands["matchResign"]> = (color) => invoke("match_resign", { color });
export const setMatchSideParams: NonNullable<MatchCommands["setMatchSideParams"]> = (side, wrn, pda) => invoke("set_match_side_params", { side, wrn, pda });
export const matchView: NonNullable<MatchCommands["matchView"]> = () => invoke("match_view");
