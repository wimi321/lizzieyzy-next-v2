import type { Backend, Settings, WireAppEvent } from "../src/api/backend";

export type {
  Backend, AppEvent, EngineProfile, EngineSetupProfile, EngineDiscovery, MoveView, Revision, Settings, EventListener,
  AnalysisSource, AnalysisSummary, AnalysisView, BoardView, CandidateView, ColorView, DocumentView,
  DocumentWarningView, EngineStatus, PointView, TreeView, WarningKindView, SubscriptionId,
  GraphPointView, GraphView,
  GameInfoView, PreviewStoneView, VariationNodeView, VariationPreviewView, VariationTreeView,
  WireAppEvent, WholeGameOptions, WholeGamePreset, WholeGameProgress, WholeGameStatus,
  WholeGameProgressListener,
  MoveRank, MoveRankEvaluationMode, MoveRankSettings, MoveLoss, ProblemMovesQuery,
  ProblemMovesView, ProblemMoveSort, ProblemMoveView, MistakeBarPoint,
} from "../src/api/backend";

type CommandName = Exclude<keyof Backend, "subscribe" | "subscribeWholeGameProgress" | "unsubscribe">;
export type MockCall = {
  [K in CommandName]: { command: K; args: Parameters<Backend[K]> }
}[CommandName];

export type MockCommand = {
  [K in CommandName]: {
    command: K;
    // Older UI scripts do not exercise move ranks. Keep those settings fixtures
    // valid without weakening the production Settings DTO or deriving analysis.
    result: K extends "settings"
      ? Omit<Settings, "move_rank"> & Partial<Pick<Settings, "move_rank">>
      : Awaited<ReturnType<Backend[K]>>;
    events?: WireAppEvent[];
    error?: string;
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
  if (next.error !== undefined) throw new Error(next.error);
  pushEvents(next.events ?? []);
  return structuredClone(next.result) as Awaited<ReturnType<Backend[K]>>;
}

export const openBytes: Backend["openBytes"] = (bytes) => command("openBytes", [bytes]);
export const openPath: Backend["openPath"] = (path) => command("openPath", [path]);
export const save: Backend["save"] = (path) => command("save", [path]);
export const play: Backend["play"] = (mv) => command("play", [mv]);
export const step: Backend["step"] = (count) => command("step", [count]);
export const goTo: Backend["goTo"] = (nodeId) => command("goTo", [nodeId]);
export const undo: Backend["undo"] = () => command("undo", []);
export const redo: Backend["redo"] = () => command("redo", []);
export const variationTree: Backend["variationTree"] = () => command("variationTree", []);
export const previewVariation: Backend["previewVariation"] = (nodeId, moves) =>
  command("previewVariation", [nodeId, moves]);
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
export const problemMoves: Backend["problemMoves"] =
  (query = { minimum_rank: "Inaccuracy", color: null, sort: "LossDesc" }) =>
    command("problemMoves", [query]);
export const setMoveRankSettings: Backend["setMoveRankSettings"] =
  (settings) => command("setMoveRankSettings", [settings]);
export const setPondering: Backend["setPondering"] = (enabled) => command("setPondering", [enabled]);
export const configureEngine: Backend["configureEngine"] = (profile) =>
  command("configureEngine", [profile]);
export const detectEngines: Backend["detectEngines"] = () => command("detectEngines", []);
export const tryConfigureEngine: Backend["tryConfigureEngine"] = (profile) =>
  command("tryConfigureEngine", [profile]);
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
