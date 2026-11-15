import { useEffect, useRef, useState, type WheelEvent } from "react";
import type { AnalysisRestriction, AnalysisView, Backend, BoardView, FloatingBoardKind, FloatingBoardsView, MoveView, PointView, VariationPreviewView } from "@/api/backend";
import { Board } from "../board/Board";
import { rankedLines } from "../board/lines";
import { SubBoard } from "../board/SubBoard";
import { useVariationPreview } from "../board/useVariationPreview";
import { type Display, DISPLAY_DEFAULTS, filled, suggestionStyle } from "../display/settings";
import { JUMP } from "../frame/BottomToolbar";
import { getMessages } from "../locales";
import type en from "../locales/en.json";
import { localeOf } from "../release/useRelease";
import { connect, createStore, useAppState } from "../state/store";
import { EDGES, useFloatingFrame } from "./frame";

type Labels = typeof en.floating;
type FloatingBackend = Pick<
  Backend,
  | "settings"
  | "subscribe"
  | "unsubscribe"
  | "subscribeFloatingBoardsChanged"
  | "play"
  | "step"
  | "togglePondering"
  | "previewVariation"
  | "addRestrictionPoints"
  | "clearAnalysisRestriction"
  | "closeFloatingBoard"
  | "setFloatingBoardOptions"
  | "dragWindow"
  | "resizeWindow"
  | "setWindowSquare"
>;

/** A wheel notch, or this much trackpad travel, is one move, as on the main board. */
const WHEEL_STEP = 40;

function report(error: unknown) {
  console.error("Floating board command failed", error);
}

/** Wheel travel turned into whole notches, by sign. */
function useNotches(onNotch: (sign: number) => void) {
  const travel = useRef(0);
  return (event: WheelEvent<HTMLElement>) => {
    travel.current += event.deltaMode === 0 ? event.deltaY : Math.sign(event.deltaY) * WHEEL_STEP;
    if (Math.abs(travel.current) < WHEEL_STEP) return;
    const sign = Math.sign(travel.current);
    travel.current = 0;
    onNotch(sign);
  };
}

/** The line of the best candidate, shown up to `length` moves by Rust's replay (captures included). */
function usePartialLine(backend: Pick<Backend, "previewVariation">, node: number | null, revision: number, moves: MoveView[] | null) {
  const [view, setView] = useState<VariationPreviewView | null>(null);
  const line = moves ? JSON.stringify(moves) : null;
  useEffect(() => {
    if (line === null || node === null) return;
    let current = true;
    backend
      .previewVariation(node, JSON.parse(line) as MoveView[])
      .then((replay) => current && setView(replay))
      .catch(report);
    return () => {
      current = false;
    };
  }, [backend, node, line]);
  return line !== null && view && view.node_id === node && view.revision === revision ? view : null;
}

export interface FloatingBoardProps {
  kind: FloatingBoardKind;
  backend: FloatingBackend;
}

/**
 * 浮动主棋盘 and 浮动小棋盘: one board filling an undecorated window of its own, in step with the
 * main window. The pointer brings up 关闭, 置顶 and 锁定; the window moves by a drag inside it and
 * is sized from its edges, always square, unless locked.
 */
export function FloatingBoard({ kind, backend }: FloatingBoardProps) {
  const [store] = useState(() => createStore());
  useEffect(() => connect(store, backend), [store, backend]);
  const document = useAppState(store, (state) => state.document);
  const analysis = useAppState(store, (state) => state.analysis);
  const revision = useAppState(store, (state) => state.revision);
  const [display, setDisplay] = useState<Display>(DISPLAY_DEFAULTS);
  const [labels, setLabels] = useState<Labels>(getMessages(navigator.language).floating);
  const [options, setOptions] = useState<FloatingBoardsView | null>(null);

  useEffect(() => {
    let active = true;
    backend
      .settings()
      .then((settings) => {
        if (!active) return;
        const shown = filled(settings.display);
        setDisplay(shown);
        setOptions(shown.floating_boards);
        setLabels(getMessages(localeOf(settings.language ?? "System", navigator.language)).floating);
      })
      .catch(report);
    let id: number | undefined;
    void backend.subscribeFloatingBoardsChanged?.((view) => active && setOptions(view)).then((subscription) => {
      id = subscription;
      if (!active) void backend.unsubscribe(subscription);
    });
    return () => {
      active = false;
      if (id !== undefined) void backend.unsubscribe(id);
    };
  }, [backend]);

  const main = kind === "Main";
  const onTop = (main ? options?.main_on_top : options?.sub_on_top) ?? false;
  const locked = (main ? options?.main_locked : options?.sub_locked) ?? false;
  const frame = useFloatingFrame(backend, locked);
  const step = (steps: number) => void backend.step(steps).catch(report);

  // The arrows, Home and End move through the game; Space pauses the analysis, as on the main board.
  useEffect(() => {
    const keys: Record<string, () => void> = {
      ArrowUp: () => step(-1),
      ArrowLeft: () => step(-1),
      ArrowDown: () => step(1),
      ArrowRight: () => step(1),
      Home: () => step(-JUMP),
      End: () => step(JUMP),
      " ": () => void backend.togglePondering().catch(report),
    };
    const onKeyDown = (event: KeyboardEvent) => {
      const run = keys[event.key];
      if (!run || event.ctrlKey || event.metaKey || event.altKey) return;
      event.preventDefault();
      run();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  const board = document?.board ?? null;
  const current = document?.tree.current ?? null;
  // While restricted, the same restricted analysis as the main window's board.
  const restriction = document?.analysis_restriction ?? null;
  const restricted = document?.restricted_analysis ?? null;
  const shown: AnalysisView | null = restriction ? (restricted && restricted.node_id === current ? restricted : null) : analysis;

  return (
    <main className="floating-board" data-testid="floating-board" data-kind={kind} data-locked={locked || undefined} data-on-top={onTop || undefined} {...frame}>
      {board ? (
        main ? (
          <MainFloating backend={backend} board={board} analysis={shown} node={current} revision={revision} display={display} komi={document?.game_info.komi ?? 0} restriction={restriction} label={labels.main} onStep={step} />
        ) : (
          <SubFloating backend={backend} board={board} analysis={shown} node={current} revision={revision} label={labels.sub} />
        )
      ) : (
        <p className="board-message">{labels.loading}</p>
      )}
      <div className="floating-buttons" role="toolbar" aria-label={main ? labels.main : labels.sub}>
        <button type="button" className="floating-button" title={labels.close} aria-label={labels.close} data-testid="floating-close" onClick={() => void backend.closeFloatingBoard(kind).catch(report)}>
          ✕
        </button>
        <button
          type="button"
          className="floating-button"
          title={onTop ? labels.unpin : labels.pin}
          aria-label={labels.pin}
          aria-pressed={onTop}
          data-testid="floating-pin"
          onClick={() => void backend.setFloatingBoardOptions(kind, !onTop, locked).then(setOptions, report)}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true">
            <path d="M4 1h4l-.6 3.4L9.5 6.5H6.6L6 11l-.6-4.5H2.5l2.1-2.1z" fill="currentColor" />
          </svg>
        </button>
        <button
          type="button"
          className="floating-button"
          title={locked ? labels.unlock : labels.lock}
          aria-label={labels.lock}
          aria-pressed={locked}
          data-testid="floating-lock"
          onClick={() => void backend.setFloatingBoardOptions(kind, onTop, !locked).then(setOptions, report)}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true">
            <rect x="2.5" y="5.5" width="7" height="5.5" rx="1" fill="currentColor" />
            <path d={locked ? "M4 5.5V4a2 2 0 0 1 4 0v1.5" : "M4 5.5V4a2 2 0 0 1 4 0"} fill="none" stroke="currentColor" strokeWidth="1.2" />
          </svg>
        </button>
      </div>
      {locked
        ? null
        : EDGES.map((edge) => (
            <span key={edge} className="floating-edge" data-edge={edge} aria-hidden="true" onPointerDown={() => void backend.resizeWindow(edge).catch(report)} />
          ))}
    </main>
  );
}

interface MainFloatingProps {
  backend: FloatingBackend;
  board: BoardView;
  analysis: AnalysisView | null;
  node: number | null;
  revision: number;
  display: Display;
  komi: number;
  restriction: AnalysisRestriction | null;
  label: string;
  onStep(steps: number): void;
}

/** The main board, as the main window shows it: candidates, a hovered line, clicks play, Alt+drag frames. */
function MainFloating({ backend, board, analysis, node, revision, display, komi, restriction, label, onStep }: MainFloatingProps) {
  const [hover, setHover] = useState<PointView | null>(null);
  const variation = useVariationPreview(backend, node, revision, analysis, hover);
  const onWheel = useNotches(onStep);
  const max = display.max_suggestions > 0 ? display.max_suggestions : board.width * board.height;
  return (
    <div className="floating-content" onWheel={onWheel}>
      <Board
        board={board}
        analysis={analysis}
        coordinates={display.show_coordinates}
        estimate={null}
        maxCandidates={max}
        label={label}
        variation={hover && display.show_hover_variation ? variation : null}
        style={suggestionStyle(display, komi)}
        restriction={restriction ? { kind: restriction.kind, points: restriction.points } : null}
        region={{
          kind: null,
          onRegion: (kind, points) => void backend.addRestrictionPoints(kind, points).catch(report),
          onClear: () => void backend.clearAnalysisRestriction().catch(report),
        }}
        onHoverCandidate={setHover}
        onPlay={(point) => void backend.play({ type: "Play", data: { color: board.to_play, point } }).catch(report)}
      />
    </div>
  );
}

/** The sub-board: the best line, the wheel showing it one move more or less. */
function SubFloating({ backend, board, analysis, node, revision, label }: { backend: FloatingBackend; board: BoardView; analysis: AnalysisView | null; node: number | null; revision: number; label: string }) {
  const best = rankedLines(analysis, node)[0] ?? null;
  const line = best?.pv ?? null;
  const lineKey = line ? `${node}:${JSON.stringify(line)}` : null;
  const [shown, setShown] = useState<{ key: string | null; length: number | null }>({ key: null, length: null });
  // A new line starts whole again.
  const length = shown.key === lineKey ? shown.length : null;
  const moves = line ? line.slice(0, length ?? line.length) : null;
  const replay = usePartialLine(backend, node, revision, moves && moves.length > 0 ? moves : null);
  const onWheel = useNotches((sign) => {
    if (!line) return;
    setShown({ key: lineKey, length: Math.max(0, Math.min(line.length, (length ?? line.length) + sign)) });
  });
  return (
    <div className="floating-content" onWheel={onWheel} data-line-length={moves?.length ?? 0}>
      <SubBoard board={board} node={node} revision={revision} variation={moves && moves.length > 0 ? replay : null} raw={moves?.length === 0} label={label} />
    </div>
  );
}
