import type { Backend, AppEvent, CommandError, EventListener, Settings } from "../src/api/backend";

export type {
  Backend, AppEvent, EngineProfile, EngineSetupProfile, EngineDiscovery, MoveView, Revision, Settings, EventListener,
  AnalysisSource, AnalysisSummary, AnalysisView, BoardView, CandidateView, ColorView, DocumentView,
  DocumentWarningView, EngineStatus, PointView, TreeView, WarningKindView, SubscriptionId,
  GraphPointView, GraphView,
  FoxAccount, FoxGame, FoxPage, FoxQuery, FoxSearchResult,
  FoxPlayer, FoxResult, FoxResultKind, FoxColor, FoxScoreUnit, FoxFailure, CommandError,
} from "../src/api/backend";

type CommandName = Exclude<keyof Backend, "subscribe" | "unsubscribe">;
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
    events?: AppEvent[];
    error?: string | CommandError;
  }
}[CommandName];

export interface MockScript {
  initialEvents?: AppEvent[];
  commands?: MockCommand[];
}

const listeners = new Map<number, EventListener>();
let nextListenerId = 0;
let pendingEvents: AppEvent[] = [];
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
export function pushEvents(events: readonly AppEvent[]): void {
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
  listeners.set(id, listener);
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
