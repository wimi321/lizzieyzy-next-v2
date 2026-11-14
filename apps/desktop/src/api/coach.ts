import { invoke } from "@tauri-apps/api/core";
import type { CoachDecision } from "./generated/CoachDecision";
import type { CoachHint } from "./generated/CoachHint";
import type { CoachMoveRated } from "./generated/CoachMoveRated";
import type { CoachSetup } from "./generated/CoachSetup";
import type { CoachSummary } from "./generated/CoachSummary";
import type { CoachView } from "./generated/CoachView";
import type { HumanSlModelStatus } from "./generated/HumanSlModelStatus";
import type { AssetDownloadStatus } from "./generated/AssetDownloadStatus";

export type { CoachDecision, CoachHint, CoachMoveRated, CoachSetup, CoachSummary, CoachView, HumanSlModelStatus };
export type { CoachError } from "./generated/CoachError";
export type { CoachRank } from "./generated/CoachRank";
export type { CoachMode } from "./generated/CoachMode";
export type { CoachColor } from "./generated/CoachColor";
export type { CoachPace } from "./generated/CoachPace";
export type { CoachVerdict } from "./generated/CoachVerdict";
export type { CoachBestMove } from "./generated/CoachBestMove";
export type { CoachWorstMove } from "./generated/CoachWorstMove";
export type { HumanSlPlay } from "./generated/HumanSlPlay";
export type CoachUpdatedListener = (view: CoachView) => void;
export type CoachMoveRatedListener = (rating: CoachMoveRated) => void;
export type CoachSummaryListener = (summary: CoachSummary) => void;

/** The Rust session owns coaching decisions, counts and model verification. */
export interface CoachCommands {
  startCoach?(setup: CoachSetup): Promise<CoachView>;
  coachView?(): Promise<CoachView | null>;
  coachHint?(): Promise<CoachHint>;
  coachDecision?(decision: CoachDecision): Promise<CoachHint>;
  coachReview?(): Promise<void>;
  humanslModelStatus?(): Promise<HumanSlModelStatus>;
  importHumanslModel?(path: string): Promise<HumanSlModelStatus>;
  pauseAssetDownload?(): Promise<AssetDownloadStatus>;
}

export const startCoach: NonNullable<CoachCommands["startCoach"]> = (setup) => invoke("start_coach", { setup });
export const coachView: NonNullable<CoachCommands["coachView"]> = () => invoke("coach_view");
export const coachHint: NonNullable<CoachCommands["coachHint"]> = () => invoke("coach_hint");
export const coachDecision: NonNullable<CoachCommands["coachDecision"]> = (decision) => invoke("coach_decision", { decision });
export const coachReview: NonNullable<CoachCommands["coachReview"]> = () => invoke("coach_review");
export const humanslModelStatus: NonNullable<CoachCommands["humanslModelStatus"]> = () => invoke("humansl_model_status");
export const importHumanslModel: NonNullable<CoachCommands["importHumanslModel"]> = (path) => invoke("import_humansl_model", { path });
export const pauseAssetDownload: NonNullable<CoachCommands["pauseAssetDownload"]> = () => invoke("pause_asset_download");
