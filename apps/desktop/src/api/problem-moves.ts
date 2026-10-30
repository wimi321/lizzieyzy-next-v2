import { invoke } from "@tauri-apps/api/core";
import type { MoveRankSettings } from "./generated/MoveRankSettings";
import type { ProblemMovesQuery } from "./generated/ProblemMovesQuery";
import type { ProblemMovesView } from "./generated/ProblemMovesView";

export type { MoveRankSettings, ProblemMovesQuery, ProblemMovesView };
export type { MoveRank } from "./generated/MoveRank";
export type { MoveRankEvaluationMode } from "./generated/MoveRankEvaluationMode";
export type { MoveLoss } from "./generated/MoveLoss";
export type { ProblemMoveSort } from "./generated/ProblemMoveSort";
export type { ProblemMoveView } from "./generated/ProblemMoveView";
export type { MistakeBarPoint } from "./generated/MistakeBarPoint";

export interface ProblemMoveCommands {
  /** Read the current line without navigating or starting analysis. Subscribe
   * before reading, and refresh on DocumentChanged, AnalysisUpdated and
   * WholeGameProgress. Discard responses from an obsolete request/revision. */
  problemMoves(query?: ProblemMovesQuery): Promise<ProblemMovesView>;
  /** Rust validates, persists and reclassifies retained results. A successful
   * change publishes DocumentChanged without changing document revision. */
  setMoveRankSettings(settings: MoveRankSettings): Promise<MoveRankSettings>;
}

export const problemMoves: ProblemMoveCommands["problemMoves"] =
  (query = { minimum_rank: "Inaccuracy", color: null, sort: "LossDesc" }) =>
    invoke("problem_moves", { query });

export const setMoveRankSettings: ProblemMoveCommands["setMoveRankSettings"] =
  (settings) => invoke("set_move_rank_settings", { settings });
