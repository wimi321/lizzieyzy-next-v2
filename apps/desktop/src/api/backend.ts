import { Channel, invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { AppEvent } from "./generated/AppEvent";
import type { EngineProfile } from "./generated/EngineProfile";
import type { MoveView } from "./generated/MoveView";
import type { Revision } from "./generated/Revision";
import type { Settings } from "./generated/Settings";
import type { SubscriptionId } from "./generated/SubscriptionId";

export type { AppEvent, EngineProfile, MoveView, Revision, Settings };
export type { AnalysisSource } from "./generated/AnalysisSource";
export type { AnalysisSummary } from "./generated/AnalysisSummary";
export type { AnalysisView } from "./generated/AnalysisView";
export type { BoardView } from "./generated/BoardView";
export type { CandidateView } from "./generated/CandidateView";
export type { ColorView } from "./generated/ColorView";
export type { DocumentView } from "./generated/DocumentView";
export type { DocumentWarningView } from "./generated/DocumentWarningView";
export type { EngineStatus } from "./generated/EngineStatus";
export type { GraphPointView } from "./generated/GraphPointView";
export type { GraphView } from "./generated/GraphView";
export type { PointView } from "./generated/PointView";
export type { TreeView } from "./generated/TreeView";
export type { WarningKindView } from "./generated/WarningKindView";
export type { SubscriptionId };
export type EventListener = (event: AppEvent) => void;

export interface Backend {
  openBytes(bytes: number[]): Promise<Revision>;
  openPath(path: string): Promise<Revision>;
  save(path: string): Promise<void>;
  play(mv: MoveView): Promise<Revision>;
  step(count: number): Promise<Revision>;
  goTo(nodeId: number): Promise<Revision>;
  undo(): Promise<Revision>;
  redo(): Promise<Revision>;
  setPondering(enabled: boolean): Promise<void>;
  configureEngine(profile: EngineProfile | null): Promise<void>;
  settings(): Promise<Settings>;
  /** Native file dialogs; null when the user cancels. */
  pickOpenPath(): Promise<string | null>;
  pickSavePath(): Promise<string | null>;
  subscribe(listener: EventListener): Promise<number>;
  unsubscribe(id: number): Promise<void>;
}

export const openBytes: Backend["openBytes"] = (bytes) => invoke("open_bytes", { bytes });
export const openPath: Backend["openPath"] = (path) => invoke("open_path", { path });
export const save: Backend["save"] = (path) => invoke("save", { path });
export const play: Backend["play"] = (mv) => invoke("play", { mv });
export const step: Backend["step"] = (count) => invoke("step", { count });
export const goTo: Backend["goTo"] = (nodeId) => invoke("go_to", { nodeId });
export const undo: Backend["undo"] = () => invoke("undo");
export const redo: Backend["redo"] = () => invoke("redo");
export const setPondering: Backend["setPondering"] = (enabled) =>
  invoke("set_pondering", { enabled });
export const configureEngine: Backend["configureEngine"] = (profile) =>
  invoke("configure_engine", { profile });
export const settings: Backend["settings"] = () => invoke("settings");

const SGF_FILTERS = [{ name: "SGF", extensions: ["sgf"] }];

export const pickOpenPath: Backend["pickOpenPath"] = async () => {
  const path = await openDialog({ multiple: false, directory: false, filters: SGF_FILTERS });
  return typeof path === "string" ? path : null;
};

export const pickSavePath: Backend["pickSavePath"] = async () =>
  (await saveDialog({ filters: SGF_FILTERS })) ?? null;

interface Connection {
  channel: Channel<AppEvent>;
  ready: Promise<SubscriptionId>;
}

let connection: Connection | undefined;
let nextListenerId = 0;
const listeners = new Map<number, EventListener>();

function connect(): Connection {
  const channel = new Channel<AppEvent>();
  const current: Connection = {
    channel,
    // Install the handler before invoking: initial snapshots may precede the reply.
    ready: Promise.resolve().then(() => invoke<SubscriptionId>("subscribe", { channel })),
  };
  channel.onmessage = (event) => {
    if (connection !== current) return;
    for (const listener of [...listeners.values()]) {
      try {
        listener(event);
      } catch (error) {
        // A failing consumer must not interrupt Tauri's ordered channel drain.
        console.error("Backend event listener failed", error);
      }
    }
  };
  return current;
}

/** All listeners share one ordered AppEvent channel. Subscribe before commands. */
export const subscribe: Backend["subscribe"] = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, listener);
  const current = connection ?? (connection = connect());
  try {
    await current.ready;
    return id;
  } catch (error) {
    listeners.delete(id);
    if (connection === current) connection = undefined;
    throw error;
  }
};

/** Idempotent. Only the final listener detaches the Rust channel. */
export const unsubscribe: Backend["unsubscribe"] = async (id) => {
  if (!listeners.delete(id) || listeners.size !== 0 || !connection) return;
  const previous = connection;
  connection = undefined;
  previous.channel.onmessage = () => {};
  const subscriptionId = await previous.ready;
  await invoke<void>("unsubscribe", { id: subscriptionId });
};
