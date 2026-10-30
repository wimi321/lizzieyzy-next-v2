import type { Backend, CommandError, DocumentView, Settings, VariationNodeView, VariationTreeView, WireAppEvent } from "../src/api/backend";

export type {
  Backend, AppEvent, EngineProfile, EngineSetupProfile, EngineDiscovery, MoveView, Revision, Settings, EventListener,
  AnalysisSource, AnalysisSummary, AnalysisView, BoardView, CandidateView, ColorView, DocumentView,
  DocumentWarningView, EngineStatus, PointView, TreeView, WarningKindView, SubscriptionId,
  GraphPointView, GraphView,
  FoxAccount, FoxGame, FoxPage, FoxQuery, FoxSearchResult,
  FoxPlayer, FoxResult, FoxResultKind, FoxColor, FoxScoreUnit, FoxFailure, CommandError,
  GameInfoView, PreviewStoneView, VariationNodeView, VariationPreviewView, VariationTreeView,
  WireAppEvent, WholeGameOptions, WholeGamePreset, WholeGameProgress, WholeGameStatus,
  WholeGameProgressListener,
} from "../src/api/backend";

type CommandName = Exclude<keyof Backend, "subscribe" | "subscribeWholeGameProgress" | "unsubscribe">;
// Existing non-Fox scenarios can omit the new history field in their fixtures.
type MockResult<K extends CommandName> = K extends "settings"
  ? Omit<Settings, "fox_history"> & Partial<Pick<Settings, "fox_history">>
  : Awaited<ReturnType<Backend[K]>>;
export type MockCall = {
  [K in CommandName]: { command: K; args: Parameters<Backend[K]> }
}[CommandName];

export type MockCommand = {
  [K in CommandName]: {
    command: K;
    result: MockResult<K>;
    events?: WireAppEvent[];
    error?: string | CommandError;
  }
}[CommandName];

export interface MockScript {
  initialEvents?: WireAppEvent[];
  commands?: MockCommand[];
}

const listeners = new Map<number, (event: WireAppEvent) => void>();
let nextListenerId = 0;
let pendingEvents: WireAppEvent[] = [];
let commands: MockCommand[] = [];
const calls: MockCall[] = [];
let dispatching = false;
/** The newest document delivered to the app, for answering unscripted read-only queries. */
let lastDocument: { revision: number; view: DocumentView } | null = null;

/** Install deterministic replies and events. No Go or analysis logic is simulated. */
export function setScript(script: MockScript): void {
  pendingEvents = structuredClone(script.initialEvents ?? []);
  commands = structuredClone(script.commands ?? []);
  calls.length = 0;
  if (listeners.size !== 0) pushEvents([]);
}

export function recordedCalls(): MockCall[] {
  return structuredClone(calls);
}

/** Push document, analysis and status events in exactly the scripted order. */
export function pushEvents(events: readonly WireAppEvent[]): void {
  pendingEvents.push(...structuredClone(events));
  if (listeners.size === 0 || dispatching) return;
  dispatching = true;
  try {
    while (listeners.size !== 0 && pendingEvents.length !== 0) {
      const event = pendingEvents.shift()!;
      if (event.type === "DocumentChanged") lastDocument = structuredClone(event.data);
      for (const listener of [...listeners.values()]) {
        try {
          listener(structuredClone(event));
        } catch (error) {
          console.error("Backend event listener failed", error);
        }
      }
    }
  } finally {
    dispatching = false;
  }
}

async function command<K extends CommandName>(
  name: K,
  args: Parameters<Backend[K]>,
): Promise<Awaited<ReturnType<Backend[K]>>> {
  calls.push(structuredClone({ command: name, args }) as MockCall);
  const next = commands[0];
  if (!next || next.command !== name) {
    throw new Error(`Expected mock command ${next?.command ?? "(none)"}, received ${name}`);
  }
  commands.shift();
  if (next.error !== undefined) {
    throw typeof next.error === "string" ? new Error(next.error) : structuredClone(next.error);
  }
  pushEvents(next.events ?? []);
  const result = next.command === "settings"
    ? { ...next.result, fox_history: next.result.fox_history ?? [] }
    : next.result;
  return structuredClone(result) as Awaited<ReturnType<Backend[K]>>;
}

export const openBytes: Backend["openBytes"] = (bytes) => command("openBytes", [bytes]);
export const openPath: Backend["openPath"] = (path) => command("openPath", [path]);
export const save: Backend["save"] = (path) => command("save", [path]);
export const play: Backend["play"] = (mv) => command("play", [mv]);
export const step: Backend["step"] = (count) => command("step", [count]);
export const goTo: Backend["goTo"] = (nodeId) => command("goTo", [nodeId]);
export const undo: Backend["undo"] = () => command("undo", []);
export const redo: Backend["redo"] = () => command("redo", []);
/**
 * Read-only queries follow the script when it lists them next; otherwise they
 * are answered from the last document, unrecorded, so a UI that fetches its
 * tree after every structural change does not disturb scripted command lists.
 */
function query<K extends "variationTree" | "previewVariation">(
  name: K,
  args: Parameters<Backend[K]>,
  fallback: () => Awaited<ReturnType<Backend[K]>>,
): Promise<Awaited<ReturnType<Backend[K]>>> {
  if (commands[0]?.command === name) return command(name, args);
  return Promise.resolve(structuredClone(fallback()));
}

/** The main line of the last document as a branchless tree; moves are unknown to the mock. */
function mainLineTree(): VariationTreeView {
  const tree = lastDocument?.view.tree;
  if (!tree) return { revision: 0, root: 0, nodes: [] };
  const nodes: VariationNodeView[] = tree.main_line.map((id, index) => ({
    id,
    parent: index === 0 ? null : (tree.main_line[index - 1] ?? null),
    children: index + 1 < tree.main_line.length ? [tree.main_line[index + 1]!] : [],
    move_number: index,
    mv: null,
    move_error: null,
  }));
  return { revision: tree.structure_revision, root: tree.root, nodes };
}

export const variationTree: Backend["variationTree"] = () => query("variationTree", [], mainLineTree);
export const previewVariation: Backend["previewVariation"] = (nodeId, moves) =>
  query("previewVariation", [nodeId, moves], () => {
    if (!lastDocument?.view.board) throw new Error("mock preview needs a document with a board");
    return { revision: lastDocument.revision, node_id: nodeId, board: lastDocument.view.board, applied_moves: [], stones: [], stop_reason: null };
  });
export const startWholeGameAnalysis: Backend["startWholeGameAnalysis"] = (options = { visits: 1000 }) =>
  command("startWholeGameAnalysis", [options]);
export const pauseWholeGameAnalysis: Backend["pauseWholeGameAnalysis"] = () =>
  command("pauseWholeGameAnalysis", []);
export const resumeWholeGameAnalysis: Backend["resumeWholeGameAnalysis"] = () =>
  command("resumeWholeGameAnalysis", []);
export const cancelWholeGameAnalysis: Backend["cancelWholeGameAnalysis"] = () =>
  command("cancelWholeGameAnalysis", []);
export const wholeGameProgress: Backend["wholeGameProgress"] = () => command("wholeGameProgress", []);
export const wholeGameResult: Backend["wholeGameResult"] = (nodeId) => command("wholeGameResult", [nodeId]);
export const setPondering: Backend["setPondering"] = (enabled) => command("setPondering", [enabled]);
export const configureEngine: Backend["configureEngine"] = (profile) =>
  command("configureEngine", [profile]);
export const detectEngines: Backend["detectEngines"] = () => command("detectEngines", []);
export const tryConfigureEngine: Backend["tryConfigureEngine"] = (profile) =>
  command("tryConfigureEngine", [profile]);
export const foxSearch: Backend["foxSearch"] = (query) => command("foxSearch", [query]);
export const foxNextPage: Backend["foxNextPage"] = (uid, cursor) => command("foxNextPage", [uid, cursor]);
export const foxOpenGame: Backend["foxOpenGame"] = (gameId) => command("foxOpenGame", [gameId]);
export const foxClearHistory: Backend["foxClearHistory"] = () => command("foxClearHistory", []);
export const settings: Backend["settings"] = () => command("settings", []);
export const pickOpenPath: Backend["pickOpenPath"] = () => command("pickOpenPath", []);
export const pickSavePath: Backend["pickSavePath"] = () => command("pickSavePath", []);
export const pickEngineFile: Backend["pickEngineFile"] = (kind) => command("pickEngineFile", [kind]);

export const subscribe: Backend["subscribe"] = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => {
    if (event.type !== "WholeGameProgress") listener(event);
  });
  pushEvents([]);
  return id;
};

export const subscribeWholeGameProgress: Backend["subscribeWholeGameProgress"] = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => {
    if (event.type === "WholeGameProgress") listener(event.data);
  });
  pushEvents([]);
  return id;
};

export const unsubscribe: Backend["unsubscribe"] = async (id) => {
  listeners.delete(id);
};

export interface MockControls {
  setScript: typeof setScript;
  pushEvents: typeof pushEvents;
  recordedCalls: typeof recordedCalls;
}

declare global {
  interface Window {
    /** Present only in the e2e build, where the Vite alias swaps in this module. */
    __mockBackend?: MockControls;
    /** Set by Playwright's addInitScript so commands sent during start-up are scripted too. */
    __mockInitialScript?: MockScript;
  }
}

if (typeof window !== "undefined") {
  window.__mockBackend = { setScript, pushEvents, recordedCalls };
  if (window.__mockInitialScript) setScript(window.__mockInitialScript);
}
