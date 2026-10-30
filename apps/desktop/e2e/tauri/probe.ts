export interface ProbeSample {
  revision: string | null;
  nodeId: string | null;
  analysisRevision: string | null;
  analysisNodeId: string | null;
  analysisText: string;
  analysisTitle: string | null;
  /** Exact root visits of the shown analysis, from data-visits. */
  analysisVisits: number | null;
  move: number | null;
  engineState: string | null;
  engineDetail: string | null;
  board: {
    width: number;
    height: number;
    cell: number;
    columns: number | null;
    rows: number | null;
  } | null;
}

export interface ProbeState {
  current: ProbeSample;
  failures: string[];
  states: { state: string; analysisVersion: number; move: number | null }[];
  analysisVersion: number;
  analysedMoves: number[];
  observedMoves: number[];
}

declare global {
  interface Window {
    __tauriE2E?: {
      observer: MutationObserver;
      collect(records: MutationRecord[]): void;
      snapshot(): ProbeState;
    };
  }
}

/** Runs in the webview through WebDriver execute; keep all helpers inside. */
export function installProbe(): ProbeState {
  window.__tauriE2E?.observer.disconnect();

  function sample(): ProbeSample {
    const byId = (id: string) => document.querySelector<HTMLElement>(`[data-testid="${id}"]`);
    const number = (text: string | null | undefined): number | null => {
      if (text == null || text.trim() === "") return null;
      const value = Number(text);
      return Number.isFinite(value) ? value : null;
    };
    const area = byId("board-area");
    const analysis = byId("analysis-move");
    const engine = byId("engine-status");
    const board = byId("board");
    const rect = board?.getBoundingClientRect();
    return {
      revision: area?.getAttribute("data-revision") ?? null,
      nodeId: area?.getAttribute("data-node-id") ?? null,
      analysisRevision: analysis?.getAttribute("data-revision") ?? null,
      analysisNodeId: analysis?.getAttribute("data-node-id") ?? null,
      analysisText: analysis?.textContent?.trim() ?? "",
      analysisTitle: analysis?.getAttribute("title") ?? null,
      analysisVisits: number(analysis?.getAttribute("data-visits")),
      move: number(byId("move-number")?.textContent),
      engineState: engine?.getAttribute("data-state") ?? null,
      engineDetail: engine?.getAttribute("title") ?? null,
      board: board && rect ? {
        width: rect.width,
        height: rect.height,
        cell: number(board.getAttribute("data-cell")) ?? 0,
        columns: number(board.getAttribute("data-columns")),
        rows: number(board.getAttribute("data-rows")),
      } : null,
    };
  }

  const state: ProbeState = {
    current: sample(),
    failures: [],
    states: [],
    analysisVersion: 0,
    analysedMoves: [],
    observedMoves: [],
  };
  let lastAnalysis: string | null = null;
  let lastEngine: string | null = null;

  function recordEngine(engine: string | null): void {
    if (engine !== null && engine !== lastEngine) {
      state.states.push({ state: engine, analysisVersion: state.analysisVersion, move: state.current.move });
      lastEngine = engine;
    }
  }

  function collect(records: MutationRecord[]): void {
    // React can update the two identities in either order within one commit.
    // Read them together after the mutation batch, never from individual records.
    const current = sample();
    state.current = current;
    if (current.move !== null && !state.observedMoves.includes(current.move)) {
      state.observedMoves.push(current.move);
    }
    const hasAnalysis = current.analysisRevision !== null || current.analysisNodeId !== null;
    const matching = current.analysisRevision !== null && current.analysisNodeId !== null
      && current.analysisRevision === current.revision && current.analysisNodeId === current.nodeId;
    if (hasAnalysis && !matching) {
      const failure = `stale analysis at move ${current.move}: board=${current.revision}/${current.nodeId}, `
        + `analysis=${current.analysisRevision}/${current.analysisNodeId} (${current.analysisText})`;
      if (!state.failures.includes(failure)) state.failures.push(failure);
    }
    const identity = current.analysisRevision !== null && current.analysisNodeId !== null
      ? JSON.stringify([current.analysisRevision, current.analysisNodeId, current.analysisText, current.analysisVisits])
      : null;
    if (identity !== null && identity !== lastAnalysis) state.analysisVersion += 1;
    lastAnalysis = identity;
    if (matching && current.move !== null && !state.analysedMoves.includes(current.move)) {
      state.analysedMoves.push(current.move);
    }

    // An entire Restarting -> Ready transition may occur between WebDriver
    // polls, even in one observer batch. oldValue preserves intermediate states.
    const changes = records.filter((record) => record.type === "attributes"
      && record.attributeName === "data-state"
      && record.target instanceof HTMLElement
      && record.target.dataset.testid === "engine-status");
    for (let index = 0; index < changes.length; index += 1) {
      const change = changes[index];
      if (!change) continue;
      const next = changes.slice(index + 1).find((record) => record.target === change.target);
      recordEngine(change.oldValue);
      recordEngine(next ? next.oldValue : (change.target as HTMLElement).getAttribute("data-state"));
    }
    recordEngine(current.engineState);
  }

  function snapshot(): ProbeState {
    return {
      ...state,
      current: { ...state.current, board: state.current.board ? { ...state.current.board } : null },
      failures: [...state.failures],
      states: state.states.map((entry) => ({ ...entry })),
      analysedMoves: [...state.analysedMoves],
      observedMoves: [...state.observedMoves],
    };
  }

  const observer = new MutationObserver(collect);
  observer.observe(document.documentElement, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeOldValue: true,
    characterData: true,
  });
  window.__tauriE2E = { observer, collect, snapshot };
  collect([]);
  return snapshot();
}

/** Flush pending DOM changes as part of the same synchronous browser command. */
export function readProbe(): ProbeState {
  const probe = window.__tauriE2E;
  if (!probe) throw new Error("Tauri DOM probe has not been installed");
  probe.collect(probe.observer.takeRecords());
  return probe.snapshot();
}

export function disconnectProbe(): void {
  window.__tauriE2E?.observer.disconnect();
  delete window.__tauriE2E;
}
