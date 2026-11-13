import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { AppEvent as WireAppEvent } from "./generated/AppEvent";
import type { ReviewCommands, WholeGameProgressListener } from "./review";
import type { BoardSyncCommands, BoardSyncStatusListener } from "./board-sync";
import type { BenchmarkCommands, BenchmarkProgressListener } from "./benchmark";
import type { AnalysisLimitCommands, AnalysisStateListener } from "./analysis-limits";
import type { ProblemMoveCommands } from "./problem-moves";
import type { StrengthCommands } from "./strength";
import type { TsumegoCommands } from "./tsumego";
import type { EditCommands } from "./edit";
import type { SetupMarkupCommands } from "./setup-markup";
import type { MoveEditingCommands } from "./move-editing";
import type { DisplaySettingsCommands } from "./display-settings";
import type { EngineCommands } from "./engines";
import type { WeightCommands, AssetDownloadProgressListener } from "./weights";
import type { AnalysisCacheCommands } from "./analysis-cache";
import type { PreferenceCommands } from "./preferences";
import type { UpdateCommands, UpdateDownloadProgressListener } from "./updates";
import type { SupportCommands } from "./support";
import type { EngineDisplayName } from "./generated/EngineDisplayName";
import type { EngineProfile } from "./generated/EngineProfile";
import type { EngineSetupProfile } from "./generated/EngineSetupProfile";
import type { EngineDiscovery } from "./generated/EngineDiscovery";
import type { MoveView } from "./generated/MoveView";
import type { Revision } from "./generated/Revision";
import type { Settings } from "./generated/Settings";
import type { SubscriptionId } from "./generated/SubscriptionId";
import type { FoxPage } from "./generated/FoxPage";
import type { FoxQuery } from "./generated/FoxQuery";
import type { FoxSearchResult } from "./generated/FoxSearchResult";
import type { VariationPreviewView } from "./generated/VariationPreviewView";
import type { VariationTreeView } from "./generated/VariationTreeView";

/** Domain status subscriptions share one ordered IPC channel while existing
 * document/analysis UI consumers retain their original event set. */
export type AppEvent = Exclude<WireAppEvent, { type: "WholeGameProgress" | "BoardSyncStatusChanged" | "BenchmarkProgress" | "AnalysisStateChanged" | "OpenFailed" | "AssetDownloadProgress" | "UpdateDownloadProgress" }>;
export type OpenFailed = Extract<WireAppEvent, { type: "OpenFailed" }>["data"];
export type OpenFailedListener = (failure: OpenFailed) => void;
export type { EngineDisplayName };
export type { WireAppEvent, EngineProfile, EngineSetupProfile, EngineDiscovery, MoveView, Revision, Settings };
export * from "./review";
export * from "./board-sync";
export * from "./benchmark";
export * from "./analysis-limits";
export * from "./problem-moves";
export * from "./strength";
export * from "./tsumego";
export * from "./edit";
export * from "./setup-markup";
export * from "./move-editing";
export * from "./display-settings";
export * from "./analysis-cache";
export * from "./engines";
export * from "./weights";
export * from "./preferences";
export * from "./updates";
export * from "./support";
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
export type { FoxPage, FoxQuery, FoxSearchResult };
export type { FoxAccount } from "./generated/FoxAccount";
export type { FoxGame } from "./generated/FoxGame";
export type { FoxPlayer } from "./generated/FoxPlayer";
export type { FoxResult } from "./generated/FoxResult";
export type { FoxResultKind } from "./generated/FoxResultKind";
export type { FoxColor } from "./generated/FoxColor";
export type { FoxScoreUnit } from "./generated/FoxScoreUnit";
export type { FoxFailure } from "./generated/FoxFailure";
export type { CommandError } from "./generated/CommandError";
export type { AutoSetupError } from "./generated/AutoSetupError";
export type { AutoSetupAttempt } from "./generated/AutoSetupAttempt";
export type EventListener = (event: AppEvent) => void;
export type EngineFileKind = "program" | "model" | "config";

export interface Backend extends ReviewCommands, BoardSyncCommands, BenchmarkCommands, AnalysisLimitCommands, ProblemMoveCommands, StrengthCommands, TsumegoCommands, EditCommands, SetupMarkupCommands, MoveEditingCommands, DisplaySettingsCommands, AnalysisCacheCommands, EngineCommands, WeightCommands, PreferenceCommands, UpdateCommands, SupportCommands {
  openBytes(bytes: number[]): Promise<Revision>;
  openPath(path: string): Promise<Revision>;
  save(path: string): Promise<void>;
  play(mv: MoveView): Promise<Revision>;
  step(count: number): Promise<Revision>;
  goTo(nodeId: number): Promise<Revision>;
  undo(): Promise<Revision>;
  redo(): Promise<Revision>;
  /** Fetch when structure_revision or comment_revision changes; match both in the reply. */
  variationTree(): Promise<VariationTreeView>;
  /** Read-only PV replay. Compare reply revision/node_id with the requested focus. */
  previewVariation(nodeId: number, moves: MoveView[]): Promise<VariationPreviewView>;
  setPondering(enabled: boolean): Promise<void>;
  engineDisplayName(profile: EngineProfile): Promise<EngineDisplayName>;
  configureEngine(profile: EngineProfile | null): Promise<void>;
  detectEngines(): Promise<EngineDiscovery>;
  /** Returns a saved profile unchanged, or discovers and starts the first usable pair. */
  autoConfigureEngine(): Promise<EngineProfile>;
  tryConfigureEngine(profile: EngineSetupProfile): Promise<void>;
  foxSearch(query: FoxQuery): Promise<FoxSearchResult>;
  foxNextPage(uid: string, cursor: string): Promise<FoxPage>;
  foxOpenGame(gameId: string): Promise<Revision>;
  foxClearHistory(): Promise<void>;
  settings(): Promise<Settings>;
  autoQuickAnalyzeOnLoad(): Promise<boolean>;
  setAutoQuickAnalyzeOnLoad(enabled: boolean): Promise<boolean>;
  /** Native file dialogs; null when the user cancels. */
  pickOpenPath(): Promise<string | null>;
  /** A folder, e.g. the Java app's, for importing its configuration. */
  pickDirectory(): Promise<string | null>;
  pickSavePath(): Promise<string | null>;
  pickEngineFile(kind: EngineFileKind): Promise<string | null>;
  /** 文件 → 退出: closing the only window ends the app. */
  quit(): Promise<void>;
  /** The native window's title, which LizzieYzy Next fills with the evaluation, players and file. */
  setWindowTitle(title: string): Promise<void>;
  subscribeOpenFailed(listener: OpenFailedListener): Promise<number>;
  subscribe(listener: EventListener): Promise<number>;
  subscribeWholeGameProgress(listener: WholeGameProgressListener): Promise<number>;
  subscribeBoardSyncStatus(listener: BoardSyncStatusListener): Promise<number>;
  subscribeBenchmarkProgress(listener: BenchmarkProgressListener): Promise<number>;
  subscribeAnalysisState(listener: AnalysisStateListener): Promise<number>;
  subscribeAssetDownloadProgress(listener: AssetDownloadProgressListener): Promise<number>;
  subscribeUpdateDownloadProgress?(listener: UpdateDownloadProgressListener): Promise<number>;
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
export const autoQuickAnalyzeOnLoad: Backend["autoQuickAnalyzeOnLoad"] =
  () => invoke("auto_quick_analyze_on_load");
export const setAutoQuickAnalyzeOnLoad: Backend["setAutoQuickAnalyzeOnLoad"] =
  (enabled) => invoke("set_auto_quick_analyze_on_load", { enabled });
export const engineDisplayName: Backend["engineDisplayName"] = (profile) =>
  invoke("engine_display_name", { profile });
export const configureEngine: Backend["configureEngine"] = (profile) =>
  invoke("configure_engine", { profile });
export const detectEngines: Backend["detectEngines"] = () => invoke("detect_engines");
export const autoConfigureEngine: Backend["autoConfigureEngine"] = () => invoke("auto_configure_engine");
export const tryConfigureEngine: Backend["tryConfigureEngine"] = (profile) =>
  invoke("try_configure_engine", { profile });
export const foxSearch: Backend["foxSearch"] = (query) => invoke("fox_search", { query });
export const foxNextPage: Backend["foxNextPage"] = (uid, cursor) =>
  invoke("fox_next_page", { uid, cursor });
export const foxOpenGame: Backend["foxOpenGame"] = (gameId) => invoke("fox_open_game", { gameId });
export const foxClearHistory: Backend["foxClearHistory"] = () => invoke("fox_clear_history");
export const settings: Backend["settings"] = () => invoke("settings");

const SGF_FILTERS = [{ name: "SGF", extensions: ["sgf"] }];
const GAME_FILTERS = [{ name: "SGF / GIB", extensions: ["sgf", "gib"] }];

export const pickOpenPath: Backend["pickOpenPath"] = async () => {
  const path = await openDialog({ multiple: false, directory: false, filters: GAME_FILTERS });
  return typeof path === "string" ? path : null;
};

export const pickDirectory: Backend["pickDirectory"] = async () => {
  const path = await openDialog({ multiple: false, directory: true });
  return typeof path === "string" ? path : null;
};

export const pickSavePath: Backend["pickSavePath"] = async () =>
  (await saveDialog({ filters: SGF_FILTERS })) ?? null;

export const quit: Backend["quit"] = () => getCurrentWindow().close();
export const setWindowTitle: Backend["setWindowTitle"] = (title) => {
  document.title = title;
  return getCurrentWindow().setTitle(title);
};

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
  if (event.type !== "WholeGameProgress" && event.type !== "BoardSyncStatusChanged" && event.type !== "BenchmarkProgress" && event.type !== "AnalysisStateChanged" && event.type !== "OpenFailed" && event.type !== "AssetDownloadProgress" && event.type !== "UpdateDownloadProgress") listener(event);
});

export const subscribeOpenFailed: Backend["subscribeOpenFailed"] =
  (listener) => subscribeWire((event) => {
    if (event.type === "OpenFailed") listener(event.data);
  });

/** Progress shares transport/lifetime with document subscriptions. Subscribe
 * before reading wholeGameProgress() for initial state. Unsubscribe its returned
 * ID with the same unsubscribe function. No second Channel opens. */
export const subscribeWholeGameProgress: Backend["subscribeWholeGameProgress"] =
  (listener) => subscribeWire((event) => {
    if (event.type === "WholeGameProgress") listener(event.data);
  });

/** Subscribe before reading boardSyncStatus(); use unsubscribe() for this ID. */
export const subscribeBoardSyncStatus: Backend["subscribeBoardSyncStatus"] =
  (listener) => subscribeWire((event) => {
    if (event.type === "BoardSyncStatusChanged") listener(event.data);
  });

/** Subscribe before reading benchmarkStatus(); use unsubscribe() for this ID. */
export const subscribeBenchmarkProgress: Backend["subscribeBenchmarkProgress"] =
  (listener) => subscribeWire((event) => {
    if (event.type === "BenchmarkProgress") listener(event.data);
  });

/** Subscribe before reading analysisState(); use unsubscribe() for this ID. */
export const subscribeAnalysisState: Backend["subscribeAnalysisState"] =
  (listener) => subscribeWire((event) => {
    if (event.type === "AnalysisStateChanged") listener(event.data);
  });

/** Subscribe before reading assetDownloadStatus(); use unsubscribe() for this ID. */
export const subscribeAssetDownloadProgress: Backend["subscribeAssetDownloadProgress"] =
  (listener) => subscribeWire((event) => {
    if (event.type === "AssetDownloadProgress") listener(event.data);
  });

/** Update progress shares the ordered channel and the unsubscribe lifecycle. */
export const subscribeUpdateDownloadProgress: NonNullable<Backend["subscribeUpdateDownloadProgress"]> =
  (listener) => subscribeWire((event) => {
    if (event.type === "UpdateDownloadProgress") listener(event.data);
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
