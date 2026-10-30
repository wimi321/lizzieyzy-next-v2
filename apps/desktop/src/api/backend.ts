import { Channel, invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { AppEvent as WireAppEvent } from "./generated/AppEvent";
import type { ReviewCommands, WholeGameProgressListener } from "./review";
import type { EngineProfile } from "./generated/EngineProfile";
import type { EngineSetupProfile } from "./generated/EngineSetupProfile";
import type { EngineDiscovery } from "./generated/EngineDiscovery";
import type { MoveView } from "./generated/MoveView";
import type { Revision } from "./generated/Revision";
import type { Settings } from "./generated/Settings";
import type { SubscriptionId } from "./generated/SubscriptionId";
import type { VariationPreviewView } from "./generated/VariationPreviewView";
import type { VariationTreeView } from "./generated/VariationTreeView";

/** Existing UI consumers receive their original event set. The review UI uses
 * subscribeWholeGameProgress; both subscriptions share one ordered IPC channel. */
export type AppEvent = Exclude<WireAppEvent, { type: "WholeGameProgress" }>;
export type { WireAppEvent, EngineProfile, EngineSetupProfile, EngineDiscovery, MoveView, Revision, Settings };
export * from "./review";
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
export type { GameInfoView } from "./generated/GameInfoView";
export type { PointView } from "./generated/PointView";
export type { PreviewStoneView } from "./generated/PreviewStoneView";
export type { TreeView } from "./generated/TreeView";
export type { VariationNodeView } from "./generated/VariationNodeView";
export type { VariationPreviewView, VariationTreeView };
export type { WarningKindView } from "./generated/WarningKindView";
export type { SubscriptionId };
export type EventListener = (event: AppEvent) => void;
export type EngineFileKind = "program" | "model" | "config";

export interface Backend extends ReviewCommands {
  openBytes(bytes: number[]): Promise<Revision>;
  openPath(path: string): Promise<Revision>;
  save(path: string): Promise<void>;
  play(mv: MoveView): Promise<Revision>;
  step(count: number): Promise<Revision>;
  goTo(nodeId: number): Promise<Revision>;
  undo(): Promise<Revision>;
  redo(): Promise<Revision>;
  /** Fetch when tree.structure_revision changes; keep only a matching reply. */
  variationTree(): Promise<VariationTreeView>;
  /** Read-only PV replay. Compare reply revision/node_id with the requested focus. */
  previewVariation(nodeId: number, moves: MoveView[]): Promise<VariationPreviewView>;
  setPondering(enabled: boolean): Promise<void>;
  configureEngine(profile: EngineProfile | null): Promise<void>;
  detectEngines(): Promise<EngineDiscovery>;
  tryConfigureEngine(profile: EngineSetupProfile): Promise<void>;
  settings(): Promise<Settings>;
  /** Native file dialogs; null when the user cancels. */
  pickOpenPath(): Promise<string | null>;
  pickSavePath(): Promise<string | null>;
  pickEngineFile(kind: EngineFileKind): Promise<string | null>;
  subscribe(listener: EventListener): Promise<number>;
  subscribeWholeGameProgress(listener: WholeGameProgressListener): Promise<number>;
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
export const variationTree: Backend["variationTree"] = () => invoke("variation_tree");
export const previewVariation: Backend["previewVariation"] = (nodeId, moves) =>
  invoke("preview_variation", { nodeId, moves });
export const setPondering: Backend["setPondering"] = (enabled) =>
  invoke("set_pondering", { enabled });
export const configureEngine: Backend["configureEngine"] = (profile) =>
  invoke("configure_engine", { profile });
export const detectEngines: Backend["detectEngines"] = () => invoke("detect_engines");
export const tryConfigureEngine: Backend["tryConfigureEngine"] = (profile) =>
  invoke("try_configure_engine", { profile });
export const settings: Backend["settings"] = () => invoke("settings");

const SGF_FILTERS = [{ name: "SGF", extensions: ["sgf"] }];

export const pickOpenPath: Backend["pickOpenPath"] = async () => {
  const path = await openDialog({ multiple: false, directory: false, filters: SGF_FILTERS });
  return typeof path === "string" ? path : null;
};

export const pickSavePath: Backend["pickSavePath"] = async () =>
  (await saveDialog({ filters: SGF_FILTERS })) ?? null;

const ENGINE_FILTERS: Record<EngineFileKind, { name: string; extensions: string[] }[] | undefined> = {
  program: undefined,
  model: [{ name: "KataGo model", extensions: ["gz", "bin", "txt"] }],
  config: [{ name: "KataGo config", extensions: ["cfg", "txt"] }],
};

export const pickEngineFile: Backend["pickEngineFile"] = async (kind) => {
  const path = await openDialog({ multiple: false, directory: false, filters: ENGINE_FILTERS[kind] });
  return typeof path === "string" ? path : null;
};

interface Connection {
  channel: Channel<WireAppEvent>;
  ready: Promise<SubscriptionId>;
}

let connection: Connection | undefined;
let nextListenerId = 0;
const listeners = new Map<number, (event: WireAppEvent) => void>();

function connect(): Connection {
  const channel = new Channel<WireAppEvent>();
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
async function subscribeWire(listener: (event: WireAppEvent) => void): Promise<number> {
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
}

export const subscribe: Backend["subscribe"] = (listener) => subscribeWire((event) => {
  if (event.type !== "WholeGameProgress") listener(event);
});

/** Progress shares transport/lifetime with document subscriptions. Subscribe
 * before reading wholeGameProgress() for initial state. Unsubscribe its returned
 * ID with the same unsubscribe function. No second Channel opens. */
export const subscribeWholeGameProgress: Backend["subscribeWholeGameProgress"] =
  (listener) => subscribeWire((event) => {
    if (event.type === "WholeGameProgress") listener(event.data);
  });

/** Idempotent. Only the final listener detaches the Rust channel. */
export const unsubscribe: Backend["unsubscribe"] = async (id) => {
  if (!listeners.delete(id) || listeners.size !== 0 || !connection) return;
  const previous = connection;
  connection = undefined;
  previous.channel.onmessage = () => {};
  const subscriptionId = await previous.ready;
  await invoke<void>("unsubscribe", { id: subscriptionId });
};
