import { invoke } from "@tauri-apps/api/core";
import type { RemoteEngineInput } from "./generated/RemoteEngineInput";
import type { Settings } from "./generated/Settings";

export type { RemoteEngineInput };
export type { RemoteSshAuth } from "./generated/RemoteSshAuth";
export type { SshAuthConfig } from "./generated/SshAuthConfig";
export type { RemoteEngineConfig } from "./generated/RemoteEngineConfig";
export type { RemoteEngineError } from "./generated/RemoteEngineError";
export type { RemoteLinkError } from "./generated/RemoteLinkError";

/** Rust owns connections, credential persistence and link/QR validation. */
export interface RemoteCommands {
  addRemoteEngine?(name: string, entry: RemoteEngineInput): Promise<Settings>;
  updateRemoteEngine?(id: string, name: string, entry: RemoteEngineInput): Promise<Settings>;
  testRemoteEngine?(entry: RemoteEngineInput): Promise<string>;
  switchToLocalEngine?(): Promise<Settings>;
  normalizeRemoteLink?(text: string): Promise<string>;
  readRemoteLinkQr?(path: string): Promise<string>;
}

export const addRemoteEngine: NonNullable<RemoteCommands["addRemoteEngine"]> = (name, entry) =>
  invoke("add_remote_engine", { name, entry });
export const updateRemoteEngine: NonNullable<RemoteCommands["updateRemoteEngine"]> = (id, name, entry) =>
  invoke("update_remote_engine", { id, name, entry });
export const testRemoteEngine: NonNullable<RemoteCommands["testRemoteEngine"]> = (entry) =>
  invoke("test_remote_engine", { entry });
export const switchToLocalEngine: NonNullable<RemoteCommands["switchToLocalEngine"]> = () =>
  invoke("switch_to_local_engine");
export const normalizeRemoteLink: NonNullable<RemoteCommands["normalizeRemoteLink"]> = (text) =>
  invoke("normalize_remote_link", { text });
export const readRemoteLinkQr: NonNullable<RemoteCommands["readRemoteLinkQr"]> = (path) =>
  invoke("read_remote_link_qr", { path });
