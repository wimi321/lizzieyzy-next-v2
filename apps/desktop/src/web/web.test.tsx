import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import type { ProblemMovesView } from "@/api/backend";
import { analysisView, boardView, candidate, graphView, midGame, midGameAnalysis } from "../../e2e/fixtures";
import type { WebClientMessage } from "../api/generated/WebClientMessage";
import type { WebServerMessage } from "../api/generated/WebServerMessage";
import type { WebTrialState } from "../api/generated/WebTrialState";
import en from "../locales/en.json";
import zhCN from "../locales/zh-CN.json";
import zhTW from "../locales/zh-TW.json";
import { DEFAULT_PREFS, loadPrefs, messagesFor, parsePrefs, savePrefs } from "./prefs";
import { analysisForSide, backoff, branchAt, branchMarks, forSide, graphForSide, INITIAL, problemsFor, reduce, socketUrl, variationOf } from "./protocol";
import { type SocketLike, useWebSocketBoard } from "./useWebSocketBoard";
import { WebPage } from "./WebPage";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

/** Node's own localStorage stands in for jsdom's and does not work here: a map does. */
function memoryStorage(): Storage {
  const store = new Map<string, string>();
  return {
    get length() {
      return store.size;
    },
    clear: () => store.clear(),
    getItem: (key) => store.get(key) ?? null,
    key: (index) => [...store.keys()][index] ?? null,
    removeItem: (key) => void store.delete(key),
    setItem: (key, value) => void store.set(key, value),
  };
}

const LOCATION = { protocol: "http:", host: "192.168.1.5:9998", pathname: "/k3y_t0ken/" };
const text = en.webBoard.page;

class FakeSocket implements SocketLike {
  onopen: SocketLike["onopen"] = null;
  onclose: SocketLike["onclose"] = null;
  onmessage: SocketLike["onmessage"] = null;
  onerror: SocketLike["onerror"] = null;
  readyState = 0;
  sent: WebClientMessage[] = [];
  closed = false;
  constructor(readonly url: string) {}
  send(data: string) {
    this.sent.push(JSON.parse(data) as WebClientMessage);
  }
  close() {
    this.closed = true;
    this.readyState = 3;
  }
  accept() {
    act(() => {
      this.readyState = 1;
      this.onopen?.(new Event("open"));
    });
  }
  push(...messages: WebServerMessage[]) {
    act(() => {
      for (const message of messages) this.onmessage?.({ data: JSON.stringify(message) } as MessageEvent);
    });
  }
  drop() {
    act(() => {
      this.readyState = 3;
      this.onclose?.({} as CloseEvent);
    });
  }
}

function sockets() {
  const opened: FakeSocket[] = [];
  const open = vi.fn((url: string) => {
    const socket = new FakeSocket(url);
    opened.push(socket);
    return socket;
  });
  return { opened, open, last: () => opened[opened.length - 1]! };
}

const hello = (overrides: Partial<Extract<WebServerMessage, { type: "Hello" }>["data"]> = {}): WebServerMessage => ({
  type: "Hello",
  data: { client_id: "c1", reconnect: "r/1", komi: 7.5, rules: "chinese", allow_trial: true, ...overrides },
});
const fullState = (nodeId = 20): WebServerMessage => ({ type: "FullState", data: { revision: 1, node_id: nodeId, board: midGame().board, analysis: midGameAnalysis(1, nodeId) } });
const trialState = (data: WebTrialState): WebServerMessage => ({ type: "TrialState", data });

function problems(): ProblemMovesView {
  const loss = (winrate_loss: number) => ({ winrate_loss, score_loss: 1, rank: "Mistake" as const, is_best: false, candidate_rank: 4, visits: 400, before_visits: 400 });
  return {
    revision: 1,
    settings: { mode: "Auto", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12] },
    query: { minimum_rank: "Inaccuracy", color: null, sort: "LossDesc" },
    entries: [
      { node_id: 31, move_number: 31, color: "Black", coordinate: "D4", loss: loss(4) },
      { node_id: 12, move_number: 12, color: "White", coordinate: "Q16", loss: loss(9) },
      { node_id: 57, move_number: 57, color: "Black", coordinate: "K10", loss: loss(9) },
    ],
    error_bar: [],
    current_index: 20,
  };
}

describe("the page's state from the server's messages", () => {
  test("a full state replaces the position; an update for it keeps the stones and replaces the analysis", () => {
    const first = reduce(reduce(INITIAL, hello()), fullState());
    expect(first.hello?.client_id).toBe("c1");
    expect(first.nodeId).toBe(20);
    const later = analysisView(2, 20, [], { visits: 9000 });
    const updated = reduce(first, { type: "AnalysisUpdate", data: { revision: 2, node_id: 20, board: { move_number: 20, to_play: "Black", captured_white: 3 }, analysis: later } });
    expect(updated.board?.stones).toBe(first.board?.stones);
    expect(updated.board?.captured_white).toBe(3);
    expect(updated.analysis?.visits).toBe(9000);
  });

  test("a late update for another position is ignored", () => {
    const state = reduce(INITIAL, fullState(20));
    const stale = reduce(state, { type: "AnalysisUpdate", data: { revision: 2, node_id: 19, board: null, analysis: analysisView(2, 19, []) } });
    expect(stale).toBe(state);
  });

  test("a refusal stays until the next trial state", () => {
    const denied = reduce(INITIAL, { type: "TrialDenied", data: { reason: "InUse" } });
    expect(denied.notice).toEqual({ kind: "denied", reason: "InUse" });
    expect(reduce(denied, { type: "TrialRejected", data: { reason: "Occupied" } }).notice).toEqual({ kind: "rejected" });
    expect(reduce(denied, trialState({ active: false })).notice).toBeNull();
  });

  test("history and problem moves are kept as sent", () => {
    const graph = graphView(30, 20, { 1: [0.5, 0] });
    const state = reduce(reduce(INITIAL, { type: "WinrateHistory", data: graph }), { type: "ProblemMoves", data: problems() });
    expect(state.history).toBe(graph);
    expect(state.problems?.entries).toHaveLength(3);
  });
});

describe("addresses and timing", () => {
  test("the socket sits beside the page under the same key, with the reconnect credential", () => {
    expect(socketUrl(LOCATION, null)).toBe("ws://192.168.1.5:9998/k3y_t0ken/ws");
    expect(socketUrl({ ...LOCATION, pathname: "/k3y_t0ken/web.html" }, "a+b/c")).toBe("ws://192.168.1.5:9998/k3y_t0ken/ws?reconnect=a%2Bb%2Fc");
    expect(socketUrl({ ...LOCATION, protocol: "https:" }, null)).toBe("wss://192.168.1.5:9998/k3y_t0ken/ws");
  });

  test("reconnecting waits 1 s, doubling to at most 30 s", () => {
    expect([0, 1, 2, 3, 4, 5, 9].map(backoff)).toEqual([1000, 2000, 4000, 8000, 16000, 30000, 30000]);
  });
});

describe("perspective", () => {
  test("White's side turns the win rate and the lead over", () => {
    expect(forSide(0.62, 3.5, true)).toEqual({ winrate: 0.62, lead: 3.5 });
    const white = forSide(0.62, 3.5, false);
    expect(white.winrate).toBeCloseTo(0.38);
    expect(white.lead).toBe(-3.5);
  });

  test("the graph and the live analysis turn over with it", () => {
    const graph = graphView(3, 1, { 0: [0.5, 0], 1: [0.7, 2] });
    const flipped = graphForSide(graph, false);
    expect(flipped.points[1]?.summary?.winrate).toBeCloseTo(0.3);
    expect(flipped.points[1]?.summary?.score_lead).toBe(-2);
    expect(flipped.points[2]?.summary).toBeNull();
    expect(graphForSide(graph, true)).toBe(graph);
    const analysis = midGameAnalysis(1, 20);
    const seen = analysisForSide(analysis, false)!;
    expect(seen.winrate).toBeCloseTo(1 - analysis.winrate);
    expect(seen.score_lead).toBe(-analysis.score_lead);
    expect(seen.summary?.winrate).toBeCloseTo(1 - analysis.winrate);
    expect(seen.candidates).toBe(analysis.candidates);
    expect(analysisForSide(null, false)).toBeNull();
  });
});

describe("a candidate's line over the position", () => {
  const board = boardView(9, 9, [[4, 4, "White"]], { toPlay: "Black" });

  test("numbered stones on empty points; an occupied point keeps its number but is not drawn", () => {
    const line = candidate({ x: 2, y: 2 }, 0, 0.5, 100, 0, [{ x: 4, y: 4 }, { x: 6, y: 6 }], board);
    const preview = variationOf(board, line, 7)!;
    expect(preview.node_id).toBe(7);
    expect(preview.stones).toEqual([
      { point: { x: 2, y: 2 }, color: "Black", move_number: 1 },
      { point: { x: 6, y: 6 }, color: "Black", move_number: 3 },
    ]);
    expect(preview.board.stones[2 * 9 + 2]).toBe("Black");
    expect(preview.board.stones[4 * 9 + 4]).toBe("White");
    expect(preview.board.to_play).toBe("White");
    expect(board.stones[2 * 9 + 2]).toBeNull();
  });

  test("an even line leaves the same side to play; no candidate, no preview", () => {
    const line = candidate({ x: 2, y: 2 }, 0, 0.5, 100, 0, [{ x: 6, y: 6 }], board);
    expect(variationOf(board, line, 7)?.board.to_play).toBe("Black");
    expect(variationOf(board, undefined, 7)).toBeNull();
  });
});

describe("问题手 on the page", () => {
  test("filtered by player and sorted here", () => {
    const view = problems();
    expect(problemsFor(view, null, "LossDesc")?.entries.map((entry) => entry.move_number)).toEqual([12, 57, 31]);
    expect(problemsFor(view, "Black", "MoveAsc")?.entries.map((entry) => entry.move_number)).toEqual([31, 57]);
    expect(problemsFor(view, null, "MoveDesc")?.entries.map((entry) => entry.move_number)).toEqual([57, 31, 12]);
    expect(view.entries.map((entry) => entry.move_number)).toEqual([31, 12, 57]);
    expect(problemsFor(null, null, "LossDesc")).toBeNull();
  });
});

describe("the trial's branches", () => {
  const trial: WebTrialState = {
    active: true,
    owner: "c1",
    branches: [
      { x: 3, y: 4, label: "1", index: 0 },
      { x: null, y: null, label: "2", index: 1 },
      { x: 5, y: 5, label: "3", index: 2 },
    ],
  };

  test("only the owner sees them, and a pass has no point", () => {
    expect(branchMarks(trial, true)).toEqual([
      { point: { x: 3, y: 4 }, kind: "Letter", text: "1" },
      { point: { x: 5, y: 5 }, kind: "Letter", text: "3" },
    ]);
    expect(branchMarks(trial, false)).toEqual([]);
    expect(branchMarks({ ...trial, active: false }, true)).toEqual([]);
  });

  test("a marked point follows its branch", () => {
    expect(branchAt(trial, 5, 5)).toBe(2);
    expect(branchAt(trial, 0, 0)).toBeNull();
    expect(branchAt(null, 5, 5)).toBeNull();
  });
});

describe("the viewer's choices", () => {
  test("damaged or unknown values fall back one by one", () => {
    expect(parsePrefs(null)).toEqual(DEFAULT_PREFS);
    expect(parsePrefs("{not json")).toEqual(DEFAULT_PREFS);
    expect(parsePrefs("[1,2]")).toEqual(DEFAULT_PREFS);
    expect(parsePrefs(JSON.stringify({ coordinates: "fox", asBlack: false, visits: "no", ownership: true }))).toEqual({ ...DEFAULT_PREFS, coordinates: "fox", asBlack: false, ownership: true });
    expect(parsePrefs(JSON.stringify({ coordinates: "roman" })).coordinates).toBe("standard");
  });

  test("kept in this browser; a refusing storage changes nothing", () => {
    const storage = memoryStorage();
    savePrefs(storage, { ...DEFAULT_PREFS, coordinates: "off", score: false });
    expect(loadPrefs(storage)).toEqual({ ...DEFAULT_PREFS, coordinates: "off", score: false });
    const refusing = {
      getItem: () => {
        throw new Error("SecurityError");
      },
      setItem: () => {
        throw new Error("QuotaExceededError");
      },
    };
    expect(loadPrefs(refusing)).toEqual(DEFAULT_PREFS);
    expect(() => savePrefs(refusing, DEFAULT_PREFS)).not.toThrow();
    expect(loadPrefs(null)).toEqual(DEFAULT_PREFS);
  });

  test("the language follows the browser: Chinese in either script, otherwise English", () => {
    expect(messagesFor(["zh-TW"])).toBe(zhTW);
    expect(messagesFor(["zh-HK", "en"])).toBe(zhTW);
    expect(messagesFor(["zh-Hant-TW"])).toBe(zhTW);
    expect(messagesFor(["zh_CN"])).toBe(zhCN);
    expect(messagesFor(["zh"])).toBe(zhCN);
    expect(messagesFor(["ja-JP", "zh-CN"])).toBe(zhCN);
    expect(messagesFor(["en-GB", "zh-CN"])).toBe(en);
    expect(messagesFor(["ko-KR"])).toBe(en);
    expect(messagesFor([])).toBe(en);
  });
});

describe("the connection", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  test("opens, receives, sends only while open, and reopens with the credential after 1 s, 2 s …", () => {
    const fake = sockets();
    const { result } = renderHook(() => useWebSocketBoard(LOCATION, fake.open));
    expect(result.current.connection).toBe("connecting");
    expect(fake.last().url).toBe("ws://192.168.1.5:9998/k3y_t0ken/ws");
    act(() => result.current.send({ type: "EnterTrial" }));
    expect(fake.last().sent).toEqual([]);

    fake.last().accept();
    expect(result.current.connection).toBe("open");
    fake.last().push(hello(), fullState());
    act(() => fake.last().onmessage?.({ data: "not json" } as MessageEvent));
    expect(result.current.state.board?.move_number).toBe(20);
    act(() => result.current.send({ type: "EnterTrial" }));
    expect(fake.last().sent).toEqual([{ type: "EnterTrial" }]);

    fake.last().drop();
    expect(result.current.connection).toBe("reconnecting");
    act(() => vi.advanceTimersByTime(999));
    expect(fake.opened).toHaveLength(1);
    act(() => vi.advanceTimersByTime(1));
    expect(fake.opened).toHaveLength(2);
    expect(fake.last().url).toBe("ws://192.168.1.5:9998/k3y_t0ken/ws?reconnect=r%2F1");
    // The state survives the drop: the page keeps showing the last position.
    expect(result.current.state.board?.move_number).toBe(20);

    fake.last().drop();
    act(() => vi.advanceTimersByTime(1999));
    expect(fake.opened).toHaveLength(2);
    act(() => vi.advanceTimersByTime(1));
    expect(fake.opened).toHaveLength(3);

    // Once through, the next drop starts from 1 s again.
    fake.last().accept();
    fake.last().drop();
    act(() => vi.advanceTimersByTime(1000));
    expect(fake.opened).toHaveLength(4);
  });

  test("leaving the page closes the socket and stops reconnecting", () => {
    const fake = sockets();
    const { unmount } = renderHook(() => useWebSocketBoard(LOCATION, fake.open));
    const first = fake.last();
    first.accept();
    unmount();
    expect(first.closed).toBe(true);
    first.drop();
    act(() => vi.advanceTimersByTime(60_000));
    expect(fake.opened).toHaveLength(1);
  });
});

describe("the page", () => {
  let storage = memoryStorage();
  beforeEach(() => {
    storage = memoryStorage();
  });
  function page() {
    const fake = sockets();
    render(<WebPage messages={en} location={LOCATION} storage={storage} open={fake.open} />);
    return fake;
  }
  const status = () => screen.getByTestId("web-status").textContent;

  test("a veil while connecting; then the position, the win rate and the rules", () => {
    const fake = page();
    expect(screen.getByTestId("web-overlay").textContent).toBe(text.connecting);
    expect(status()).toBe(text.noGame);
    expect(screen.getByTestId<HTMLButtonElement>("web-trial-enter").disabled).toBe(true);
    fake.last().accept();
    fake.last().push(hello(), fullState());
    expect(screen.queryByTestId("web-overlay")).toBeNull();
    expect(screen.getByTestId("board")).toBeTruthy();
    for (const part of ["Move 20", "Black to play", "Black win rate 56.2%", "Black lead +1.6", "Visits 3.6k", "Komi 7.5", "Rules chinese"]) expect(status()).toContain(part);
    fake.last().drop();
    expect(screen.getByTestId("web-overlay").textContent).toBe(text.reconnecting);
  });

  test("White's perspective turns the numbers over and is remembered", () => {
    const fake = page();
    fake.last().accept();
    fake.last().push(hello(), fullState());
    fireEvent.click(screen.getByTestId("web-perspective-white"));
    expect(status()).toContain("White win rate 43.8%");
    expect(status()).toContain("White lead −1.6");
    expect(screen.getByTestId("web-perspective-white").getAttribute("aria-checked")).toBe("true");
    fireEvent.change(screen.getByTestId("web-coordinates"), { target: { value: "fox" } });
    fireEvent.click(screen.getByTestId("web-line-visits"));
    expect(loadPrefs(storage)).toEqual({ ...DEFAULT_PREFS, asBlack: false, coordinates: "fox", visits: false });
  });

  test("the saved choices come back", () => {
    savePrefs(storage, { ...DEFAULT_PREFS, coordinates: "withI", asBlack: false, ownership: true });
    page();
    expect(screen.getByTestId<HTMLSelectElement>("web-coordinates").value).toBe("withI");
    expect(screen.getByTestId("web-perspective-black").getAttribute("aria-checked")).toBe("false");
    expect(screen.getByTestId<HTMLInputElement>("web-ownership").checked).toBe(true);
  });

  test("trying moves: asking, the owner's buttons, and someone else's trial", () => {
    const fake = page();
    const socket = fake.last();
    socket.accept();
    socket.push(hello(), fullState());
    fireEvent.click(screen.getByTestId("web-trial-enter"));
    expect(socket.sent).toEqual([{ type: "EnterTrial" }]);

    socket.push(trialState({ active: true, owner: "c1", can_back: false, can_forward: true, branches: [] }));
    expect(screen.getByTestId("web-trial").dataset.state).toBe("mine");
    expect(screen.getByTestId<HTMLButtonElement>("web-trial-back").disabled).toBe(true);
    expect(screen.getByTestId<HTMLButtonElement>("web-trial-anchor").disabled).toBe(true);
    fireEvent.click(screen.getByTestId("web-trial-forward"));
    socket.push(trialState({ active: true, owner: "c1", can_back: true, can_forward: false, branches: [] }));
    fireEvent.click(screen.getByTestId("web-trial-back"));
    fireEvent.click(screen.getByTestId("web-trial-anchor"));
    fireEvent.click(screen.getByTestId("web-trial-exit"));
    expect(socket.sent.slice(1)).toEqual([{ type: "TrialForward" }, { type: "TrialBack" }, { type: "TrialToAnchor" }, { type: "ExitTrial" }]);

    socket.push(trialState({ active: true, owner: "c2" }));
    expect(screen.getByTestId("web-trial").dataset.state).toBe("others");
    expect(screen.getByTestId("web-trial-others").textContent).toBe(text.othersTrial);
    expect(screen.queryByTestId("web-trial-enter")).toBeNull();

    socket.push(trialState({ active: false }));
    expect(screen.getByTestId("web-trial").dataset.state).toBe("none");
    socket.push(hello({ allow_trial: false }));
    expect(screen.queryByTestId("web-trial-enter")).toBeNull();
  });

  test("a refusal is shown for a few seconds", () => {
    vi.useFakeTimers();
    const fake = page();
    fake.last().accept();
    fake.last().push(hello(), fullState(), { type: "TrialDenied", data: { reason: "HostBusy" } });
    expect(screen.getByTestId("web-notice").textContent).toBe(text.denied.HostBusy);
    act(() => vi.advanceTimersByTime(3999));
    expect(screen.getByTestId("web-notice")).toBeTruthy();
    act(() => vi.advanceTimersByTime(1));
    expect(screen.queryByTestId("web-notice")).toBeNull();
    fake.last().push({ type: "TrialRejected", data: { reason: "Occupied" } });
    expect(screen.getByTestId("web-notice").textContent).toBe(text.rejected);
  });

  test("the graph and the problem moves appear once the host sends them", () => {
    const fake = page();
    fake.last().accept();
    fake.last().push(hello(), fullState());
    expect(screen.queryByTestId("web-graph")).toBeNull();
    expect(screen.queryByTestId("web-problems")).toBeNull();
    fake.last().push({ type: "WinrateHistory", data: graphView(30, 20, { 1: [0.5, 0], 2: [0.55, 1] }) }, { type: "ProblemMoves", data: problems() });
    // The plot itself waits for a measured width, which jsdom never reports; Playwright draws it.
    expect(screen.getByTestId("winrate-graph").dataset.count).toBe("30");
    expect(screen.getByTestId("web-problems").textContent).toContain("Q16");
  });
});
