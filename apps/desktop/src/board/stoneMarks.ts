import type { BoardView } from "@/api/backend";
import { type Context, disc, text, UI_FONT } from "./canvas";
import { type Geometry, pointIndex, pointToPixel } from "./geometry";
import { RANK_MARK_RADIUS, type RankedStone } from "./moveMarks";
import { RANK_COLORS } from "./suggestion";

function centre(geometry: Geometry, board: BoardView, index: number) {
  return pointToPixel(geometry, { x: index % board.width, y: Math.floor(index / board.width) });
}

/**
 * `drawMoveNumbers`: each label on its stone in the other colour. The last move's number is red,
 * with Java's small red corner in place of the ring.
 */
export function drawMoveNumbers(context: Context, geometry: Geometry, board: BoardView, labels: readonly (string | null)[]) {
  const r = geometry.stoneRadius;
  const last = board.last_move?.type === "Play" ? pointIndex(board.width, board.last_move.data.point) : -1;
  labels.forEach((label, index) => {
    const stone = board.stones[index];
    if (!label || !stone) return;
    const { x, y } = centre(geometry, board, index);
    if (index === last) {
      const far = (r * 16) / 15;
      const near = (r * 4) / 11;
      context.fillStyle = "rgb(255, 0, 0)";
      context.beginPath();
      context.moveTo(x - far, y - far);
      context.lineTo(x - far, y - near);
      context.lineTo(x - near, y - far);
      context.closePath();
      context.fill();
    } else {
      context.fillStyle = stone === "Black" ? "#fff" : "#000";
    }
    text(context, label, x, y, UI_FONT, r * 1.4, label.length >= 3 ? r * 1.85 : r * 1.4);
  });
}

/** `drawMoveRankMarkCircle` for the moves before the last: a dot in the rank's colour, bigger for worse moves. */
export function drawRankDots(context: Context, geometry: Geometry, board: BoardView, stones: readonly RankedStone[]) {
  for (const { index, rank } of stones) {
    const { x, y } = centre(geometry, board, index);
    context.fillStyle = RANK_COLORS[rank];
    disc(context, x, y, Math.round(geometry.cell * RANK_MARK_RADIUS[rank]));
    context.fill();
  }
}
