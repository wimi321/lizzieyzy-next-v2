import { invoke } from "@tauri-apps/api/core";
import type { CaptureSettings } from "./generated/CaptureSettings";
import type { CapturePoint } from "./generated/CapturePoint";
import type { CaptureRecognition } from "./generated/CaptureRecognition";
import type { CaptureResult } from "./generated/CaptureResult";
import type { CapturePermission } from "./generated/CapturePermission";
import type { ScreenCaptureStarted } from "./generated/ScreenCaptureStarted";
import type { AppEvent } from "./generated/AppEvent";
import type { Revision } from "./generated/Revision";

export type { CaptureSettings, CapturePoint, CaptureRecognition, CaptureResult, CapturePermission, ScreenCaptureStarted };
export type { CaptureCorner } from "./generated/CaptureCorner";
export type { CaptureCell } from "./generated/CaptureCell";
export type { CaptureCells } from "./generated/CaptureCells";
export type { CaptureGrid } from "./generated/CaptureGrid";
export type { CaptureSession } from "./generated/CaptureSession";
export type { CaptureOutcome } from "./generated/CaptureOutcome";
export type { CaptureError } from "./generated/CaptureError";
export type { ScreenCaptureError } from "./generated/ScreenCaptureError";
export type CaptureEndedListener = (event: Extract<AppEvent, { type: "CaptureEnded" }>["data"]) => void;

export interface CaptureCommands {
  captureSettings?(): Promise<CaptureSettings>;
  setCaptureSettings?(settings: CaptureSettings): Promise<CaptureSettings>;
  capturePermission?(): Promise<CapturePermission>;
  requestCapturePermission?(): Promise<CapturePermission>;
  openCapturePermissionSettings?(): Promise<void>;
  startScreenCapture?(): Promise<ScreenCaptureStarted>;
  /** Browser-displayable PNG, received directly as binary IPC. */
  captureImage?(id: number): Promise<ArrayBuffer>;
  /** Points are in screenshot pixels; the UI does not fit or recognize a grid. */
  recognizeCapture?(id: number, points: [CapturePoint, CapturePoint, CapturePoint]): Promise<CaptureRecognition>;
  finishCapture?(id: number, result: CaptureResult): Promise<Revision>;
  cancelCapture?(id: number): Promise<void>;
}

export const captureSettings: NonNullable<CaptureCommands["captureSettings"]> = () =>
  invoke("capture_settings");
export const setCaptureSettings: NonNullable<CaptureCommands["setCaptureSettings"]> = (settings) =>
  invoke("set_capture_settings", { settings });
export const capturePermission: NonNullable<CaptureCommands["capturePermission"]> = () =>
  invoke("capture_permission");
export const requestCapturePermission: NonNullable<CaptureCommands["requestCapturePermission"]> = () =>
  invoke("request_capture_permission");
export const openCapturePermissionSettings: NonNullable<CaptureCommands["openCapturePermissionSettings"]> = () =>
  invoke("open_capture_permission_settings");
export const startScreenCapture: NonNullable<CaptureCommands["startScreenCapture"]> = () =>
  invoke("start_screen_capture");
export const captureImage: NonNullable<CaptureCommands["captureImage"]> = (id) =>
  invoke("capture_image", { id });
export const recognizeCapture: NonNullable<CaptureCommands["recognizeCapture"]> = (id, points) =>
  invoke("recognize_capture", { id, points });
export const finishCapture: NonNullable<CaptureCommands["finishCapture"]> = (id, result) =>
  invoke("finish_capture", { id, result });
export const cancelCapture: NonNullable<CaptureCommands["cancelCapture"]> = (id) =>
  invoke("cancel_capture", { id });
