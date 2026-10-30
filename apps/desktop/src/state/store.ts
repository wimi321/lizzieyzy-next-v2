import { useSyncExternalStore } from "react";
import type { AnalysisView, AppEvent, Backend, DocumentView, EngineStatus } from "@/api/backend";

export interface AppState {
  /** Revision of the latest DocumentChanged; -1 before the first one. */
  revision: number;
  document: DocumentView | null;
  /** Only ever analysis of the current node at the current revision. */
  analysis: AnalysisView | null;
  engine: EngineStatus;
}

export const initialState: AppState = {
  revision: -1,
  document: null,
  analysis: null,
  engine: { type: "Stopped" },
};

/** Backend events are the only input. Stale analysis is dropped here, once. */
export function reduce(state: AppState, event: AppEvent): AppState {
  switch (event.type) {
    case "DocumentChanged": {
      const { revision, view } = event.data;
      if (revision < state.revision) return state;
      const analysis = matches(state.analysis, revision, view) ? state.analysis : null;
      return { ...state, revision, document: view, analysis };
    }
    case "AnalysisUpdated": {
      if (!state.document || !matches(event.data, state.revision, state.document)) return state;
      return { ...state, analysis: event.data };
    }
    case "EngineStatusChanged":
      return { ...state, engine: event.data };
  }
}

function matches(analysis: AnalysisView | null, revision: number, view: DocumentView): boolean {
  return analysis !== null && analysis.revision === revision && analysis.node_id === view.tree.current;
}

export interface Store {
  getState(): AppState;
  subscribe(listener: () => void): () => void;
  dispatch(event: AppEvent): void;
}

export function createStore(initial: AppState = initialState): Store {
  let state = initial;
  const listeners = new Set<() => void>();
  return {
    getState: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    dispatch(event) {
      const next = reduce(state, event);
      if (next === state) return;
      state = next;
      for (const listener of [...listeners]) listener();
    },
  };
}

/** Feeds backend events into the store. Returns a disposer for effect cleanup. */
export function connect(store: Store, backend: Pick<Backend, "subscribe" | "unsubscribe">): () => void {
  let disposed = false;
  let id: number | undefined;
  backend
    .subscribe((event) => store.dispatch(event))
    .then((subscription) => {
      id = subscription;
      if (disposed) void backend.unsubscribe(subscription);
    })
    .catch((error: unknown) => console.error("Backend subscription failed", error));
  return () => {
    disposed = true;
    if (id !== undefined) void backend.unsubscribe(id);
  };
}

export function useAppState<T>(store: Store, select: (state: AppState) => T): T {
  return useSyncExternalStore(store.subscribe, () => select(store.getState()));
}
