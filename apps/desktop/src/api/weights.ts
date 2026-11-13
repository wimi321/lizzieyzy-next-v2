import { invoke } from "@tauri-apps/api/core";
import type { AccelerationPlan } from "./generated/AccelerationPlan";
import type { AssetDownloadStatus } from "./generated/AssetDownloadStatus";
import type { DiagnosticRepair } from "./generated/DiagnosticRepair";
import type { EngineDiagnostics } from "./generated/EngineDiagnostics";
import type { OfficialWeight } from "./generated/OfficialWeight";
import type { Settings } from "./generated/Settings";

export type { AccelerationPlan, AssetDownloadStatus, DiagnosticRepair, EngineDiagnostics, OfficialWeight };
export type { AccelerationReason } from "./generated/AccelerationReason";
export type { DownloadPhase } from "./generated/DownloadPhase";
export type { DiagnosticCheck } from "./generated/DiagnosticCheck";
export type { DiagnosticKind } from "./generated/DiagnosticKind";
export type { DiagnosticStatus } from "./generated/DiagnosticStatus";
export type AssetDownloadProgressListener = (status: AssetDownloadStatus) => void;

/** Optional until the weight management UI is connected. IDs and repair kinds
 * are interpreted by Rust; the frontend does not choose paths or download URLs. */
export interface WeightCommands {
  officialWeights?(): Promise<OfficialWeight[]>;
  startAssetDownload?(assetId: string): Promise<AssetDownloadStatus>;
  cancelAssetDownload?(): Promise<AssetDownloadStatus>;
  assetDownloadStatus?(): Promise<AssetDownloadStatus>;
  accelerationPlan?(): Promise<AccelerationPlan>;
  useOfficialWeight?(assetId: string): Promise<Settings>;
  diagnoseEngine?(): Promise<EngineDiagnostics>;
  repairEngine?(repair: DiagnosticRepair): Promise<EngineDiagnostics>;
}

export const officialWeights: NonNullable<WeightCommands["officialWeights"]> = () => invoke("official_weights");
export const startAssetDownload: NonNullable<WeightCommands["startAssetDownload"]> = (assetId) => invoke("start_asset_download", { assetId });
export const cancelAssetDownload: NonNullable<WeightCommands["cancelAssetDownload"]> = () => invoke("cancel_asset_download");
export const assetDownloadStatus: NonNullable<WeightCommands["assetDownloadStatus"]> = () => invoke("asset_download_status");
export const accelerationPlan: NonNullable<WeightCommands["accelerationPlan"]> = () => invoke("acceleration_plan");
export const useOfficialWeight: NonNullable<WeightCommands["useOfficialWeight"]> = (assetId) => invoke("use_official_weight", { assetId });
export const diagnoseEngine: NonNullable<WeightCommands["diagnoseEngine"]> = () => invoke("diagnose_engine");
export const repairEngine: NonNullable<WeightCommands["repairEngine"]> = (repair) => invoke("repair_engine", { repair });
