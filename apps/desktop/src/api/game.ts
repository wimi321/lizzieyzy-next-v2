import { invoke } from "@tauri-apps/api/core";
import type { AiPlayer } from "./generated/AiPlayer";
import type { ColorView } from "./generated/ColorView";
import type { GameSetup } from "./generated/GameSetup";
import type { GameView } from "./generated/GameView";
import type { PvPick } from "./generated/PvPick";
import type { Revision } from "./generated/Revision";

export type { AiPlayer, GameSetup, GameView, PvPick };
export type { GameStart } from "./generated/GameStart";
export type { GamePhase } from "./generated/GamePhase";
export type { GameEndReason } from "./generated/GameEndReason";
export type { GameError } from "./generated/GameError";
export type { Player } from "./generated/Player";
export type { KataGoPlay } from "./generated/KataGoPlay";
export type { HumanClock } from "./generated/HumanClock";
export type { HumanClockView } from "./generated/HumanClockView";
export type { RandomOpening } from "./generated/RandomOpening";
export type { ResignRule } from "./generated/ResignRule";
export type GameUpdatedListener = (view: GameView) => void;

/** The Rust actor owns moves, clocks, analysis and persistence. */
export interface GameCommands {
  startGame?(setup: GameSetup): Promise<GameView>;
  pauseGame?(): Promise<GameView>;
  resumeGame?(): Promise<GameView>;
  stopGame?(): Promise<GameView>;
  resignGame?(): Promise<GameView>;
  takeBack?(): Promise<GameView>;
  continueGame?(aiColor?: ColorView | null): Promise<GameView>;
  setAiPlayer?(color: ColorView, player: AiPlayer): Promise<GameView | null>;
  playBestMove?(pv?: PvPick | null): Promise<Revision>;
  gameView?(): Promise<GameView | null>;
}

export const startGame: NonNullable<GameCommands["startGame"]> = (setup) => invoke("start_game", { setup });
export const pauseGame: NonNullable<GameCommands["pauseGame"]> = () => invoke("pause_game");
export const resumeGame: NonNullable<GameCommands["resumeGame"]> = () => invoke("resume_game");
export const stopGame: NonNullable<GameCommands["stopGame"]> = () => invoke("stop_game");
export const resignGame: NonNullable<GameCommands["resignGame"]> = () => invoke("resign_game");
export const takeBack: NonNullable<GameCommands["takeBack"]> = () => invoke("take_back");
export const continueGame: NonNullable<GameCommands["continueGame"]> = (aiColor = null) => invoke("continue_game", { aiColor });
export const setAiPlayer: NonNullable<GameCommands["setAiPlayer"]> = (color, player) => invoke("set_ai_player", { color, player });
export const playBestMove: NonNullable<GameCommands["playBestMove"]> = (pv = null) => invoke("play_best_move", { pv });
export const gameView: NonNullable<GameCommands["gameView"]> = () => invoke("game_view");
