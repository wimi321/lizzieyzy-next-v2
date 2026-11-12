import { type MouseEvent, useEffect, useMemo, useRef, useState } from "react";
import type { AnalysisView, BoardView, MarkupView, MoveRank, PointView, VariationPreviewView } from "@/api/backend";
import { useBoardAssets, useElementSize } from "./assets";
import { drawGoban, drawOverlay, drawStones, type Names, type NextMove, prepare } from "./draw";
import { computeGeometry, pixelToPoint, pointIndex } from "./geometry";
import { drawMarkup } from "./markup";

export interface BoardProps {
  board: BoardView;
  analysis: AnalysisView | null;
  coordinates: boolean;
  ownership: boolean;
  maxCandidates: number;
  label: string;
  /** A candidate picked outside the board, e.g. a table row; the pointer still wins. */
  preview?: PointView | null;
  /** Rust's replay of the previewed candidate's variation, captures included. */
  variation?: VariationPreviewView | null;
  /** Player names under the board; null when the game has none. */
  names?: Names | null;
  /** The last move's rank, when it and the move before it are analysed. */
  lastRank?: MoveRank | null;
  /** Recorded continuations of this position, main line first. */
  next?: NextMove[];
  passLabel?: string;
  /** Hint for an empty game; shown only while there is no analysis to show. */
  emptyHint?: string | null;
  /** The current move's triangles, squares, circles, crosses and labels. */
  markup?: readonly MarkupView[];
  /**
   * A setup or markup tool is in hand: every click, on a stone or not, goes to `onPoint`, and
   * candidates show no variation.
   */
  editing?: boolean;
  onPoint?(point: PointView): void;
  /** The candidate under the pointer, so the app can ask Rust for its variation. */
  onHoverCandidate?(point: PointView | null): void;
  onPlay(point: PointView): void;
}

const NO_MARKUP: readonly MarkupView[] = [];

const NO_NEXT: NextMove[] = [];

/**
 * Three stacked canvases: goban, stones, analysis. They are laid out in device pixels, as Java
 * does, so lines and stones land on whole pixels at any scale. Each layer redraws only when its
 * own inputs change; analysis frames are coalesced per animation frame.
 */
export function Board({
  board,
  analysis,
  coordinates,
  ownership,
  maxCandidates,
  label,
  preview = null,
  variation = null,
  names = null,
  lastRank = null,
  next = NO_NEXT,
  passLabel = "Pass",
  emptyHint = null,
  markup = NO_MARKUP,
  editing = false,
  onPoint,
  onHoverCandidate,
  onPlay,
}: BoardProps) {
  const container = useRef<HTMLDivElement>(null);
  const gobanCanvas = useRef<HTMLCanvasElement>(null);
  const stoneCanvas = useRef<HTMLCanvasElement>(null);
  const overlayCanvas = useRef<HTMLCanvasElement>(null);
  const size = useElementSize(container);
  const { images, fonts } = useBoardAssets();
  const [hover, setHover] = useState<PointView | null>(null);

  const big = coordinates || names !== null;
  const geometry = useMemo(
    () => computeGeometry(board.width, board.height, size.deviceWidth, size.deviceHeight, size.dpr, big),
    [board.width, board.height, size, big],
  );
  // Canvas pixels per CSS pixel, as laid out: the pointer and E2E speak CSS pixels.
  const scale = size.width > 0 ? size.deviceWidth / size.width : 1;

  // With a tool in hand the board shows the game itself, never a candidate's variation.
  const target = editing ? null : (hover ?? preview);
  // The replay replaces the stones while a candidate is previewed.
  const shown = target && variation ? variation : null;
  const stones = shown ? shown.board : board;

  useEffect(() => {
    const canvas = gobanCanvas.current;
    const context = canvas && images && prepare(canvas, geometry);
    if (context && images) drawGoban(context, geometry, images, { coordinates, names, toPlay: board.to_play, dpr: geometry.dpr });
  }, [geometry, images, fonts, coordinates, names, board.to_play]);

  useEffect(() => {
    const canvas = stoneCanvas.current;
    const context = canvas && images && prepare(canvas, geometry);
    if (context && images) drawStones(context, geometry, images, stones);
  }, [geometry, images, stones]);

  useEffect(() => {
    if (geometry.cell <= 0 || !images) return;
    const frame = requestAnimationFrame(() => {
      const canvas = overlayCanvas.current;
      const context = canvas && prepare(canvas, geometry);
      if (!context) return;
      drawOverlay(context, geometry, images, {
        board,
        analysis,
        ownership,
        maxCandidates,
        hover: target,
        variation: shown,
        lastRank,
        next,
        passLabel,
        emptyHint,
        dpr: geometry.dpr,
      });
      // Java hides the markup while a variation is shown.
      if (!shown) drawMarkup(context, geometry, images, board, markup);
    });
    return () => cancelAnimationFrame(frame);
  }, [geometry, images, fonts, board, analysis, ownership, maxCandidates, target, shown, lastRank, next, passLabel, emptyHint, markup]);

  const pointAt = (event: MouseEvent<HTMLElement>): PointView | null => {
    const rect = event.currentTarget.getBoundingClientRect();
    return pixelToPoint(geometry, (event.clientX - rect.left) * scale, (event.clientY - rect.top) * scale);
  };

  const onMouseMove = (event: MouseEvent<HTMLElement>) => {
    if (editing) return;
    const point = pointAt(event);
    const candidate = point && analysis?.candidates.some((c) => c.point?.x === point.x && c.point?.y === point.y);
    const nextHover = candidate ? point : null;
    if (hover?.x === nextHover?.x && hover?.y === nextHover?.y) return;
    setHover(nextHover);
    onHoverCandidate?.(nextHover);
  };

  const onMouseLeave = () => {
    if (hover === null) return;
    setHover(null);
    onHoverCandidate?.(null);
  };

  const onClick = (event: MouseEvent<HTMLElement>) => {
    const point = pointAt(event);
    if (!point) return;
    if (editing) onPoint?.(point);
    else if (!board.stones[pointIndex(board.width, point)]) onPlay(point);
  };

  return (
    <div
      ref={container}
      className="board"
      role="img"
      aria-label={label}
      data-testid="board"
      data-columns={board.width}
      data-rows={board.height}
      data-ready={images && fonts > 0 ? "true" : "false"}
      data-editing={editing || undefined}
      // What the analysis layer was given, so E2E can wait for it before comparing pixels.
      data-last-rank={lastRank ?? ""}
      data-next-moves={next.length}
      // Layout for E2E pointer tests: CSS pixels of point (0, 0) and the line spacing.
      data-cell={(geometry.cell / scale).toFixed(3)}
      data-origin-x={(geometry.originX / scale).toFixed(3)}
      data-origin-y={(geometry.originY / scale).toFixed(3)}
      onMouseMove={onMouseMove}
      onMouseLeave={onMouseLeave}
      onClick={onClick}
    >
      <canvas ref={gobanCanvas} className="board-layer" />
      <canvas ref={stoneCanvas} className="board-layer" />
      <canvas ref={overlayCanvas} className="board-layer" data-testid="board-analysis" />
    </div>
  );
}
