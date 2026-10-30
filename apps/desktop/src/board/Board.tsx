import { type MouseEvent, useEffect, useMemo, useRef, useState } from "react";
import type { AnalysisView, BoardView, PointView, VariationPreviewView } from "@/api/backend";
import { type AnalysisLayer, drawAnalysis, drawBackground, drawPreviewLabels, drawStones, prepare } from "./draw";
import { backingSize, computeGeometry, pixelToPoint, pointIndex } from "./geometry";
import { type Palette, readPalette } from "./palette";

export interface BoardProps {
  board: BoardView;
  analysis: AnalysisView | null;
  coordinates: boolean;
  ownership: boolean;
  maxCandidates: number;
  label: string;
  /** A candidate picked outside the board, e.g. a table row; the pointer still wins. */
  preview?: PointView | null;
  /**
   * Rust's replay of the previewed candidate's variation, captures included.
   * Until it arrives the board overlays the variation on the current stones.
   */
  variation?: VariationPreviewView | null;
  /** The candidate under the pointer, so the app can ask Rust for its variation. */
  onHoverCandidate?(point: PointView | null): void;
  onPlay(point: PointView): void;
}

interface Size {
  width: number;
  height: number;
  dpr: number;
}

/**
 * Three stacked canvases: wood and grid, stones, analysis. Each layer redraws
 * only when its own inputs change; analysis frames are coalesced per animation
 * frame so a 20 Hz stream never queues work.
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
  onHoverCandidate,
  onPlay,
}: BoardProps) {
  const container = useRef<HTMLDivElement>(null);
  const backgroundCanvas = useRef<HTMLCanvasElement>(null);
  const stoneCanvas = useRef<HTMLCanvasElement>(null);
  const analysisCanvas = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState<Size>({ width: 0, height: 0, dpr: 1 });
  const [palette, setPalette] = useState<Palette>(() => readPalette(null));
  const [hover, setHover] = useState<PointView | null>(null);

  useEffect(() => {
    const element = container.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    // ResizeObserver reports the initial size too, so no synchronous measuring here.
    const observer = new ResizeObserver(() => {
      const rect = element.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      setSize((previous) =>
        previous.width === rect.width && previous.height === rect.height && previous.dpr === dpr
          ? previous
          : { width: rect.width, height: rect.height, dpr },
      );
      setPalette(readPalette(element));
    });
    observer.observe(element);
    const scheme = window.matchMedia?.("(prefers-color-scheme: dark)");
    const refreshPalette = () => setPalette(readPalette(element));
    scheme?.addEventListener("change", refreshPalette);
    return () => {
      observer.disconnect();
      scheme?.removeEventListener("change", refreshPalette);
    };
  }, []);

  const geometry = useMemo(
    () => computeGeometry(board.width, board.height, size.width, size.height, size.dpr, coordinates),
    [board.width, board.height, size, coordinates],
  );
  const backing = useMemo(() => backingSize(geometry), [geometry]);

  useEffect(() => {
    const canvas = backgroundCanvas.current;
    const context = canvas && prepare(canvas, geometry, backing.width, backing.height);
    if (context) drawBackground(context, geometry, palette, coordinates);
  }, [geometry, backing, palette, coordinates]);

  const target = hover ?? preview;
  // The replay replaces the stones while a candidate is previewed; its last move is numbered instead of ringed.
  const shown = target && variation ? variation : null;
  const stones = useMemo(() => (shown ? { ...shown.board, last_move: null } : board), [shown, board]);

  useEffect(() => {
    const canvas = stoneCanvas.current;
    const context = canvas && prepare(canvas, geometry, backing.width, backing.height);
    if (context) drawStones(context, geometry, palette, stones);
  }, [geometry, backing, palette, stones]);

  useEffect(() => {
    if (geometry.cell <= 0) return;
    const layer: AnalysisLayer = { board, analysis, ownership, maxCandidates, hover: target };
    const frame = requestAnimationFrame(() => {
      const canvas = analysisCanvas.current;
      const context = canvas && prepare(canvas, geometry, backing.width, backing.height);
      if (!context) return;
      if (shown) drawPreviewLabels(context, geometry, palette, shown.stones);
      else drawAnalysis(context, geometry, palette, layer);
    });
    return () => cancelAnimationFrame(frame);
  }, [geometry, backing, palette, board, analysis, ownership, maxCandidates, target, shown]);

  const pointAt = (event: MouseEvent<HTMLElement>): PointView | null => {
    const rect = event.currentTarget.getBoundingClientRect();
    return pixelToPoint(geometry, event.clientX - rect.left, event.clientY - rect.top);
  };

  const onMouseMove = (event: MouseEvent<HTMLElement>) => {
    const point = pointAt(event);
    const candidate = point && analysis?.candidates.some((c) => c.point?.x === point.x && c.point?.y === point.y);
    const next = candidate ? point : null;
    if (hover?.x === next?.x && hover?.y === next?.y) return;
    setHover(next);
    onHoverCandidate?.(next);
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
      // Layout for E2E pointer tests: CSS pixels of point (0, 0) and the line spacing.
      data-cell={geometry.cell.toFixed(3)}
      data-origin-x={geometry.originX.toFixed(3)}
      data-origin-y={geometry.originY.toFixed(3)}
      onMouseMove={onMouseMove}
      onMouseLeave={onMouseLeave}
      onClick={onClick}
    >
      <canvas ref={backgroundCanvas} className="board-layer" />
      <canvas ref={stoneCanvas} className="board-layer" />
      <canvas ref={analysisCanvas} className="board-layer" data-testid="board-analysis" />
    </div>
  );
}
