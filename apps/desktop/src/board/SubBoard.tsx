import { useEffect, useMemo, useRef, useState } from "react";
import type { BoardView, VariationPreviewView } from "@/api/backend";
import { useBoardAssets, useElementSize } from "./assets";
import { drawGoban, drawLine, drawStones, prepare } from "./draw";
import { computeGeometry } from "./geometry";

export interface SubBoardProps {
  board: BoardView;
  /** The node and document revision on display; a replay of anything else is stale. */
  node: number | null;
  revision: number;
  /** Rust's replay of the line to show: the best move's, or the hovered candidate's. */
  variation: VariationPreviewView | null;
  label: string;
}

/**
 * LizzieYzy Next's sub-board: the line the engine expects, played out and numbered, so the main
 * board can stay on the game. Without a line it shows the position itself.
 */
export function SubBoard({ board, node, revision, variation: latest, label }: SubBoardProps) {
  // The engine keeps deepening its line, and every change is replayed anew. Until the next
  // replay arrives the last one of this position stays up, so the board does not blink.
  const [kept, setKept] = useState(latest);
  if (latest && latest !== kept) setKept(latest);
  const variation = latest ?? (kept && kept.node_id === node && kept.revision === revision ? kept : null);
  const container = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const size = useElementSize(container);
  const { images, fonts } = useBoardAssets();
  const geometry = useMemo(
    () => computeGeometry(board.width, board.height, size.deviceWidth, size.deviceHeight, size.dpr, false),
    [board.width, board.height, size],
  );

  useEffect(() => {
    const context = canvas.current && images && prepare(canvas.current, geometry);
    if (!context || !images) return;
    drawGoban(context, geometry, images, { coordinates: false, names: null, toPlay: board.to_play, dpr: geometry.dpr });
    drawStones(context, geometry, images, variation ? variation.board : board);
    if (variation) drawLine(context, geometry, images, board, variation);
  }, [geometry, images, fonts, board, variation]);

  return (
    <div ref={container} className="sub-board" role="img" aria-label={label} data-testid="sub-board" data-moves={variation?.stones.length ?? 0}>
      <canvas ref={canvas} className="board-layer" />
    </div>
  );
}
