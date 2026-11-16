import { type PointerEvent, useEffect, useRef, useState } from "react";
import type { ColorView, PointView, ProblemMoveSort } from "@/api/backend";
import { Board } from "../board/Board";
import { formatScore, formatVisits, formatWinrate } from "../board/format";
import { DEFAULT_SUGGESTION_STYLE } from "../board/suggestionText";
import { DISPLAY_DEFAULTS } from "../display/settings";
import { indexAtX } from "../graph/scale";
import { WinrateGraph } from "../graph/WinrateGraph";
import { ProblemList } from "../review/ProblemList";
import { type WebMessages, type WebPrefs, loadPrefs, savePrefs } from "./prefs";
import { analysisForSide, branchAt, branchMarks, forSide, graphForSide, problemsFor, variationOf } from "./protocol";
import { type SocketFactory, useWebSocketBoard } from "./useWebSocketBoard";

const fill = (text: string, values: Record<string, string | number>) => text.replace(/\{(\w+)\}/g, (whole, key: string) => (key in values ? String(values[key]) : whole));
const LONG_PRESS = 500;
const NOTICE_MS = 4000;

export interface WebPageProps {
  messages: WebMessages;
  location: Pick<Location, "protocol" | "host" | "pathname">;
  storage: Storage | null;
  /** Tests replace the browser's WebSocket. */
  open?: SocketFactory;
}

/** The point under a touch, from the board's own layout attributes. */
function pointUnder(element: HTMLElement | null, clientX: number, clientY: number): PointView | null {
  const board = element?.querySelector<HTMLElement>('[data-testid="board"]');
  if (!board) return null;
  const cell = Number(board.dataset.cell);
  const rect = board.getBoundingClientRect();
  if (!(cell > 0)) return null;
  const x = Math.round((clientX - rect.left - Number(board.dataset.originX)) / cell);
  const y = Math.round((clientY - rect.top - Number(board.dataset.originY)) / cell);
  const columns = Number(board.dataset.columns);
  const rows = Number(board.dataset.rows);
  return x >= 0 && y >= 0 && x < columns && y < rows ? { x, y } : null;
}

/** The move under the pointer on the win rate graph, from the graph's own layout attributes. */
function graphIndexUnder(element: HTMLElement, clientX: number, count: number): number | null {
  const graph = element.querySelector<HTMLElement>('[data-testid="winrate-graph"]');
  if (!graph || count === 0) return null;
  const rect = graph.getBoundingClientRect();
  const layout = { width: rect.width, height: rect.height, left: Number(graph.dataset.plotLeft), right: Number(graph.dataset.plotRight), top: 0, bottom: 0, count };
  return indexAtX(layout, clientX - rect.left);
}

/**
 * LizzieYzy's web viewer (P3-33; Java's Web 旁观): the host's board and analysis over P3-32's
 * WebSocket, with the win rate graph, the problem moves and one viewer's trial at a time.
 */
export function WebPage({ messages, location, storage, open }: WebPageProps) {
  const text = messages.webBoard.page;
  const { state, connection, send } = useWebSocketBoard(location, open);
  const [prefs, setPrefs] = useState<WebPrefs>(() => loadPrefs(storage));
  const [hover, setHover] = useState<PointView | null>(null);
  const [pressed, setPressed] = useState<PointView | null>(null);
  const [problemColor, setProblemColor] = useState<ColorView | null>(null);
  const [problemSort, setProblemSort] = useState<ProblemMoveSort>("LossDesc");
  const [expired, setExpired] = useState<object | null>(null);
  /** The move the viewer points at on the graph, while the host stays on `at`: its values show instead. */
  const [pointed, setPointed] = useState<{ index: number; at: number } | null>(null);
  const area = useRef<HTMLDivElement>(null);
  const press = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const swallow = useRef(false);

  useEffect(() => {
    if (!state.notice) return;
    const notice = state.notice;
    const timer = setTimeout(() => setExpired(notice), NOTICE_MS);
    return () => clearTimeout(timer);
  }, [state.notice]);

  const change = (update: Partial<WebPrefs>) =>
    setPrefs((previous) => {
      const next = { ...previous, ...update };
      savePrefs(storage, next);
      return next;
    });

  const { board, analysis, trial, hello } = state;
  const me = hello?.client_id ?? null;
  const mine = trial?.active === true && me !== null && trial.owner === me;
  const othersTrial = trial?.active === true && !mine;
  const target = hover ?? pressed;
  const candidate = target ? analysis?.candidates.find((entry) => entry.point?.x === target.x && entry.point?.y === target.y) : undefined;
  const variation = board && candidate ? variationOf(board, candidate, state.nodeId ?? 0) : null;
  const side = prefs.asBlack ? text.black : text.white;
  const seen = analysis ? forSide(analysis.winrate, analysis.score_lead, prefs.asBlack) : null;
  const notice = state.notice && state.notice !== expired ? state.notice : null;
  const isCandidate = (point: PointView | null) => point !== null && (analysis?.candidates.some((entry) => entry.point?.x === point.x && entry.point?.y === point.y) ?? false);

  const releasePress = () => {
    clearTimeout(press.current);
    setPressed(null);
  };
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    // A long press the browser took for a scroll never got its click: nothing is left to swallow.
    swallow.current = false;
    if (event.pointerType !== "touch") return;
    const point = pointUnder(area.current, event.clientX, event.clientY);
    clearTimeout(press.current);
    press.current = setTimeout(() => {
      if (!isCandidate(point)) return;
      // The click that follows lifting the finger shows nothing and plays nothing.
      swallow.current = true;
      setPressed(point);
    }, LONG_PRESS);
  };

  const onPlay = (point: PointView) => {
    if (swallow.current) {
      swallow.current = false;
      return;
    }
    if (!mine) return;
    const branch = branchAt(trial, point.x, point.y);
    send(branch === null ? { type: "TrialMove", data: { x: point.x, y: point.y } } : { type: "TrialChild", data: { index: branch } });
  };

  const history = state.history;
  const inspected = history && pointed?.at === history.current_index ? pointed.index : null;
  const inspect = (event: PointerEvent<HTMLDivElement>) => {
    const index = history ? graphIndexUnder(event.currentTarget, event.clientX, history.points.length) : null;
    setPointed(history && index !== null ? { index, at: history.current_index } : null);
  };
  const shownGraph = history ? { ...graphForSide(history, prefs.asBlack), current_index: inspected ?? history.current_index } : null;

  const status = board
    ? [
        fill(text.moveNumber, { n: board.move_number }),
        fill(text.toPlay, { color: board.to_play === "Black" ? text.black : text.white }),
        ...(seen && analysis
          ? [
              fill(text.winrate, { color: side, value: `${formatWinrate(seen.winrate)}%` }),
              fill(text.score, { color: side, value: formatScore(seen.lead) }),
              fill(text.visits, { value: formatVisits(analysis.visits) }),
            ]
          : []),
        ...(hello?.komi !== undefined ? [fill(text.komi, { value: hello.komi })] : []),
        ...(hello?.rules ? [fill(text.rules, { value: hello.rules })] : []),
      ]
    : [text.noGame];

  return (
    <div className="web-page" data-testid="web-page" data-connection={connection}>
      <header className="web-header">
        <h1>{text.title}</h1>
        <p className="web-status" data-testid="web-status">
          {status.map((part) => (
            <span key={part}>{part}</span>
          ))}
        </p>
      </header>
      <main className="web-main">
        <div className="web-board" ref={area} onPointerDown={onPointerDown} onPointerUp={releasePress} onPointerCancel={releasePress} onPointerLeave={releasePress}>
          {board ? (
            <Board
              board={board}
              analysis={analysis}
              coordinates={prefs.coordinates !== "off"}
              coordinateStyle={prefs.coordinates === "off" ? undefined : prefs.coordinates}
              estimate={prefs.ownership && analysis?.ownership ? { style: DISPLAY_DEFAULTS.estimate_style, values: analysis.ownership } : null}
              maxCandidates={DISPLAY_DEFAULTS.max_suggestions}
              label={messages.board.label}
              passLabel={messages.board.pass}
              preview={pressed}
              variation={variation}
              markup={branchMarks(trial, mine)}
              style={{ ...DEFAULT_SUGGESTION_STYLE, winrate: prefs.winrate, visits: prefs.visits, score: prefs.score, komi: hello?.komi ?? 0 }}
              onHoverCandidate={setHover}
              onPlay={onPlay}
            />
          ) : null}
        </div>
        <aside className="web-side">
          <section className="web-card web-trial" data-testid="web-trial" data-state={mine ? "mine" : othersTrial ? "others" : "none"}>
            {mine ? (
              <>
                <p className="web-hint">{text.trialHint}</p>
                <div className="web-buttons">
                  <button type="button" data-testid="web-trial-back" disabled={!trial?.can_back} onClick={() => send({ type: "TrialBack" })}>
                    {text.back}
                  </button>
                  <button type="button" data-testid="web-trial-forward" disabled={!trial?.can_forward} onClick={() => send({ type: "TrialForward" })}>
                    {text.forward}
                  </button>
                  <button type="button" data-testid="web-trial-anchor" disabled={!trial?.can_back} onClick={() => send({ type: "TrialToAnchor" })}>
                    {text.anchor}
                  </button>
                  <button type="button" className="web-primary" data-testid="web-trial-exit" onClick={() => send({ type: "ExitTrial" })}>
                    {text.exitTrial}
                  </button>
                </div>
              </>
            ) : othersTrial ? (
              <p className="web-hint" data-testid="web-trial-others">
                {text.othersTrial}
              </p>
            ) : hello?.allow_trial === false ? null : (
              <button type="button" className="web-primary" data-testid="web-trial-enter" disabled={connection !== "open" || !board} onClick={() => send({ type: "EnterTrial" })}>
                {text.trial}
              </button>
            )}
            {notice ? (
              <p className="web-notice" role="status" data-testid="web-notice">
                {notice.kind === "denied" ? text.denied[notice.reason] : text.rejected}
              </p>
            ) : null}
          </section>
          <section className="web-card web-settings" data-testid="web-settings">
            <h2>{text.settings}</h2>
            <label className="web-field">
              <span>{text.coordinates}</span>
              <select data-testid="web-coordinates" value={prefs.coordinates} onChange={(event) => change({ coordinates: event.target.value as WebPrefs["coordinates"] })}>
                {(["off", "standard", "withI", "fox"] as const).map((style) => (
                  <option key={style} value={style}>
                    {text.coordinateStyles[style]}
                  </option>
                ))}
              </select>
            </label>
            <div className="web-field" role="radiogroup" aria-label={text.perspective}>
              <span>{text.perspective}</span>
              {([true, false] as const).map((asBlack) => (
                <button key={String(asBlack)} type="button" role="radio" aria-checked={prefs.asBlack === asBlack} data-testid={`web-perspective-${asBlack ? "black" : "white"}`} onClick={() => change({ asBlack })}>
                  {asBlack ? text.perspectiveBlack : text.perspectiveWhite}
                </button>
              ))}
            </div>
            <div className="web-field">
              <span>{text.lines}</span>
              {(["winrate", "visits", "score"] as const).map((line) => (
                <label key={line} className="web-check">
                  <input type="checkbox" data-testid={`web-line-${line}`} checked={prefs[line]} onChange={(event) => change({ [line]: event.target.checked })} />
                  {line === "winrate" ? text.lineWinrate : line === "visits" ? text.lineVisits : text.lineScore}
                </label>
              ))}
              <label className="web-check">
                <input type="checkbox" data-testid="web-ownership" checked={prefs.ownership} onChange={(event) => change({ ownership: event.target.checked })} />
                {text.ownership}
              </label>
            </div>
          </section>
          {shownGraph ? (
            <section className="web-card web-graph" data-testid="web-graph">
              <h2>{text.graphTitle}</h2>
              {/* A mouse shows a move's values by hovering, a finger by touching or sliding. */}
              <div
                className="web-graph-plot"
                data-inspected={inspected ?? undefined}
                onPointerDown={inspect}
                onPointerMove={(event) => (event.pointerType === "mouse" || event.buttons !== 0) && inspect(event)}
                onPointerLeave={(event) => event.pointerType === "mouse" && setPointed(null)}
              >
                <WinrateGraph
                  graph={shownGraph}
                  analysis={analysisForSide(analysis, prefs.asBlack)}
                  labels={{ ...messages.graph, label: text.graphTitle, legendWinrate: prefs.asBlack ? text.legendBlack : text.legendWhite }}
                  onJump={() => undefined}
                />
              </div>
            </section>
          ) : null}
          {state.problems ? (
            <section className="web-card web-problems" data-testid="web-problems">
              <h2>{text.problemsTitle}</h2>
              <ProblemList
                view={problemsFor(state.problems, problemColor, problemSort)}
                current={state.nodeId ?? -1}
                color={problemColor}
                sort={problemSort}
                analysed={history?.points.some((point) => point.summary !== null) ?? false}
                labels={messages.problems}
                onColor={setProblemColor}
                onSort={setProblemSort}
                onJump={() => undefined}
              />
            </section>
          ) : null}
        </aside>
      </main>
      {connection !== "open" ? (
        <div className="web-overlay" role="status" data-testid="web-overlay">
          <span>{connection === "connecting" ? text.connecting : text.reconnecting}</span>
        </div>
      ) : null}
    </div>
  );
}
