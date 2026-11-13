import { invoke } from "@tauri-apps/api/core";
import type { UpdateCheck } from "./generated/UpdateCheck";
import type { UpdateDownloadStatus } from "./generated/UpdateDownloadStatus";

export type { UpdateCheck, UpdateDownloadStatus };
export type { UpdatePhase } from "./generated/UpdatePhase";
export type UpdateDownloadProgressListener = (status: UpdateDownloadStatus) => void;

/** The backend owns release selection, signature checks and installation. */
export interface UpdateCommands {
  checkUpdate(): Promise<UpdateCheck>;
  /** The daily check at start: a new version, or null (not due, switched off, unsigned build or failed). */
  automaticUpdateCheck(): Promise<UpdateCheck | null>;
  downloadUpdate(): Promise<UpdateDownloadStatus>;
  cancelUpdateDownload(): Promise<UpdateDownloadStatus>;
  updateDownloadStatus(): Promise<UpdateDownloadStatus>;
  installUpdate(): Promise<UpdateDownloadStatus>;
}

export const checkUpdate: UpdateCommands["checkUpdate"] = () => invoke("check_update");
export const automaticUpdateCheck: UpdateCommands["automaticUpdateCheck"] = () => invoke("automatic_update_check");
export const downloadUpdate: UpdateCommands["downloadUpdate"] = () => invoke("download_update");
export const cancelUpdateDownload: UpdateCommands["cancelUpdateDownload"] = () => invoke("cancel_update_download");
export const updateDownloadStatus: UpdateCommands["updateDownloadStatus"] = () => invoke("update_download_status");
export const installUpdate: UpdateCommands["installUpdate"] = () => invoke("install_update");
