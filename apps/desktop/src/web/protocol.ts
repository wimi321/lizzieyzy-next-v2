import type { AnalysisView, BoardView, CandidateView, ColorView, GraphView, MarkupView, PreviewStoneView, ProblemMoveSort, ProblemMovesView, VariationPreviewView } from "@/api/backend";
import type { WebHello } from "../api/generated/WebHello";
import type { WebServerMessage } from "../api/generated/WebServerMessage";
import type { WebTrialDeniedReason } from "../api/generated/WebTrialDeniedReason";
import type { WebTrialState } from "../api/generated/WebTrialState";

/** What the browser page knows: the latest of each server message (P3-32's protocol). */
export interface WebState {
  hello: WebHello | null;
  nodeId: number | null;
  board: BoardView | null;
  analysis: AnalysisView | null;
  history: GraphView | null;
  problems: ProblemMovesView | null;
  trial: WebTrialState | null;
  /** The last refusal, until the next trial state or a new one. */
  notice: { kind: "denied"; reason: WebTrialDeniedReason } | { kind: "rejected" } | null;
}

export const INITIAL: WebState = { hello: null, nodeId: null, board: null, analysis: null, history: null, problems: null, trial: null, notice: null };

export function reduce(state: WebState, message: WebServerMessage): WebState {
  switch (message.type) {
    case "Hello":
      return { ...state, hello: message.data };
    case "FullState":
      return { ...state, nodeId: message.data.node_id ?? null, board: message.data.board ?? null, analysis: message.data.analysis ?? null };
    case "AnalysisUpdate": {
      // A late update for another position waits for that position's FullState.
      if (message.data.node_id !== state.nodeId) return state;
      const position = message.data.board;
      const board = state.board && position ? { ...state.board, ...withoutUndefined(position) } : state.board;
      return { ...state, board, analysis: message.data.analysis ?? null };
    }
    case "WinrateHistory":
      return { ...state, history: message.data };
    case "ProblemMoves":
      return { ...state, problems: message.data };
    case "TrialState":
      return { ...state, trial: message.data, notice: null };
    case "TrialDenied":
      return { ...state, notice: { kind: "denied", reason: message.data.reason } };
    case "TrialRejected":
      return { ...state, notice: { kind: "rejected" } };
  }
}

function withoutUndefined<T extends object>(value: T): Partial<T> {
  return Object.fromEntries(Object.entries(value).filter(([, entry]) => entry !== undefined)) as Partial<T>;
}

/** The page is served at `/<token>/`; its socket is `/<token>/ws`, with the reconnect credential. */
export function socketUrl(location: Pick<Location, "protocol" | "host" | "pathname">, reconnect: string | null): string {
  const directory = location.pathname.slice(0, location.pathname.lastIndexOf("/") + 1);
  const scheme = location.protocol === "https:" ? "wss:" : "ws:";
  return `${scheme}//${location.host}${directory}ws${reconnect ? `?reconnect=${encodeURIComponent(reconnect)}` : ""}`;
}

/** Java's page: 1 s, doubling, at most 30 s. */
export function backoff(attempt: number): number {
  return Math.min(30_000, 1000 * 2 ** Math.max(0, attempt));
}

/** The win rate (0–1) and lead as the chosen side sees them; Rust always sends Black's. */
export function forSide(blackWinrate: number, blackLead: number, asBlack: boolean): { winrate: number; lead: number } {
  return asBlack ? { winrate: blackWinrate, lead: blackLead } : { winrate: 1 - blackWinrate, lead: -blackLead };
}

/** The history graph from White's side: each point's win rate and lead turned over. */
export function graphForSide(graph: GraphView, asBlack: boolean): GraphView {
  if (asBlack) return graph;
  return {
    ...graph,
    points: graph.points.map((point) =>
      point.summary ? { ...point, summary: { ...point.summary, winrate: 1 - point.summary.winrate, score_lead: -point.summary.score_lead } } : point,
    ),
  };
}

export function analysisForSide(analysis: AnalysisView | null, asBlack: boolean): AnalysisView | null {
  if (!analysis || asBlack) return analysis;
  const summary = analysis.summary ? { ...analysis.summary, winrate: 1 - analysis.summary.winrate, score_lead: -analysis.summary.score_lead } : analysis.summary;
  return { ...analysis, winrate: 1 - analysis.winrate, score_lead: -analysis.score_lead, summary };
}

/**
 * A candidate's line as numbered stones over the position, as Java's web page draws it. The
 * page has no Rust to replay it, so nothing is captured: a move onto an occupied point keeps
 * its index but is not drawn.
 */
export function variationOf(board: BoardView, candidate: CandidateView | undefined, nodeId: number): VariationPreviewView | null {
  if (!candidate?.point) return null;
  const stones = [...board.stones];
  const placed: PreviewStoneView[] = [];
  candidate.pv.forEach((move, index) => {
    if (move.type !== "Play") return;
    const { point, color } = move.data;
    const at = point.y * board.width + point.x;
    if (point.x < 0 || point.y < 0 || point.x >= board.width || point.y >= board.height || stones[at]) return;
    stones[at] = color;
    placed.push({ point, color, move_number: index + 1 });
  });
  const mover: ColorView = candidate.pv.length % 2 === 0 ? board.to_play : board.to_play === "Black" ? "White" : "Black";
  return {
    revision: 0,
    node_id: nodeId,
    board: { ...board, stones, to_play: mover },
    applied_moves: candidate.pv,
    stones: placed,
    stop_reason: null,
  };
}

/** The 问题手 list filtered by player and sorted here: the server sends the default query. */
export function problemsFor(view: ProblemMovesView | null, color: ColorView | null, sort: ProblemMoveSort): ProblemMovesView | null {
  if (!view) return null;
  const entries = view.entries.filter((entry) => color === null || entry.color === color);
  entries.sort((a, b) =>
    sort === "LossDesc" ? b.loss.winrate_loss - a.loss.winrate_loss || a.move_number - b.move_number : sort === "MoveDesc" ? b.move_number - a.move_number : a.move_number - b.move_number,
  );
  return { ...view, entries };
}

/** The trial's branches as labels on their points (Java's numbered markers). */
export function branchMarks(trial: WebTrialState | null, mine: boolean): MarkupView[] {
  if (!trial?.active || !mine) return [];
  return (trial.branches ?? []).flatMap((branch) =>
    branch.x === null || branch.x === undefined || branch.y === null || branch.y === undefined ? [] : [{ point: { x: branch.x, y: branch.y }, kind: "Letter" as const, text: branch.label ?? "" }],
  );
}

/** The branch to follow when the owner clicks a marked point. */
export function branchAt(trial: WebTrialState | null, x: number, y: number): number | null {
  const branch = (trial?.branches ?? []).find((entry) => entry.x === x && entry.y === y);
  return branch?.index ?? null;
}
