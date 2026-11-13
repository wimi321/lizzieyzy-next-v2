import { invoke } from "@tauri-apps/api/core";
import type { UpdateCheck } from "./generated/UpdateCheck";
import type { UpdateDownloadStatus } from "./generated/UpdateDownloadStatus";

export type { UpdateCheck, UpdateDownloadStatus };
export type { UpdatePhase } from "./generated/UpdatePhase";
export type UpdateDownloadProgressListener = (status: UpdateDownloadStatus) => void;

/** The backend owns release selection, signature checks and installation. */
export interface UpdateCommands {
  checkUpdate?(): Promise<UpdateCheck>;
  downloadUpdate?(): Promise<UpdateDownloadStatus>;
  cancelUpdateDownload?(): Promise<UpdateDownloadStatus>;
  updateDownloadStatus?(): Promise<UpdateDownloadStatus>;
  installUpdate?(): Promise<UpdateDownloadStatus>;
}

export const checkUpdate: NonNullable<UpdateCommands["checkUpdate"]> = () => invoke("check_update");
export const downloadUpdate: NonNullable<UpdateCommands["downloadUpdate"]> = () => invoke("download_update");
export const cancelUpdateDownload: NonNullable<UpdateCommands["cancelUpdateDownload"]> = () => invoke("cancel_update_download");
export const updateDownloadStatus: NonNullable<UpdateCommands["updateDownloadStatus"]> = () => invoke("update_download_status");
export const installUpdate: NonNullable<UpdateCommands["installUpdate"]> = () => invoke("install_update");
