import { type MouseEvent, useEffect, useMemo, useRef, useState } from "react";
import type { AnalysisView, BoardView, MoveRank, PointView, VariationPreviewView } from "@/api/backend";
import { useBoardAssets, useElementSize } from "./assets";
import { drawGoban, drawOverlay, drawStones, type Names, type NextMove, prepare } from "./draw";
import { computeGeometry, pixelToPoint, pointIndex } from "./geometry";

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
  /** The candidate under the pointer, so the app can ask Rust for its variation. */
  onHoverCandidate?(point: PointView | null): void;
  onPlay(point: PointView): void;
}

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
    () => computeGeometry(board.width, board.height, Math.round(size.width * size.dpr), Math.round(size.height * size.dpr), size.dpr, big),
    [board.width, board.height, size, big],
  );

  const target = hover ?? preview;
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
    });
    return () => cancelAnimationFrame(frame);
  }, [geometry, images, fonts, board, analysis, ownership, maxCandidates, target, shown, lastRank, next, passLabel, emptyHint]);

  const pointAt = (event: MouseEvent<HTMLElement>): PointView | null => {
    const rect = event.currentTarget.getBoundingClientRect();
    return pixelToPoint(geometry, (event.clientX - rect.left) * geometry.dpr, (event.clientY - rect.top) * geometry.dpr);
  };

  const onMouseMove = (event: MouseEvent<HTMLElement>) => {
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
    if (point && !board.stones[pointIndex(board.width, point)]) onPlay(point);
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
      // What the analysis layer was given, so E2E can wait for it before comparing pixels.
      data-last-rank={lastRank ?? ""}
      data-next-moves={next.length}
      // Layout for E2E pointer tests: CSS pixels of point (0, 0) and the line spacing.
      data-cell={(geometry.cell / geometry.dpr).toFixed(3)}
      data-origin-x={(geometry.originX / geometry.dpr).toFixed(3)}
      data-origin-y={(geometry.originY / geometry.dpr).toFixed(3)}
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
