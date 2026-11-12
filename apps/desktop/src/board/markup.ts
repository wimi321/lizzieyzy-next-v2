import type { BoardView, ColorView, MarkupView } from "@/api/backend";
import { type BoardImages, disc, text, UI_FONT, wood } from "./draw";
import { type Geometry, pointIndex, pointToPixel } from "./geometry";

type Context = CanvasRenderingContext2D;

/** Java's `markupStroke`: white on a black stone, black on a white stone or the wood. */
export function markColor(stone: ColorView | null | undefined): string {
  return stone === "Black" ? "#fff" : "#000";
}

/** Java's sizes, from the stone radius: the shapes sit well inside a stone. */
export function markSize(kind: MarkupView["kind"], r: number): number {
  switch (kind) {
    case "Triangle":
      return Math.trunc(((r + 1) * 2) / 3);
    case "Circle":
      return Math.trunc((r * 2) / 3);
    default:
      return Math.trunc((r + 1) / 2);
  }
}

/**
 * The current move's markup (`BoardRenderer.drawStoneMarkup`): triangles, squares, circles,
 * crosses and letter or number labels. A label on an empty point sits on a patch of wood, so the
 * grid does not cross it.
 */
export function drawMarkup(context: Context, geometry: Geometry, images: BoardImages, board: BoardView, markup: readonly MarkupView[]) {
  if (geometry.cell <= 0 || markup.length === 0) return;
  const r = geometry.stoneRadius;
  context.save();
  context.lineJoin = "round";
  context.lineCap = "round";
  context.lineWidth = Math.max(1.5, r / 9);
  for (const mark of markup) {
    if (mark.point.x >= board.width || mark.point.y >= board.height) continue;
    const stone = board.stones[pointIndex(board.width, mark.point)];
    const { x, y } = pointToPixel(geometry, mark.point);
    const color = markColor(stone);
    const size = markSize(mark.kind, r);
    context.strokeStyle = color;
    context.beginPath();
    switch (mark.kind) {
      case "Triangle": {
        const offset = (1.5 * size) / Math.sqrt(3);
        context.moveTo(x, y - size);
        context.lineTo(x - offset, y + size / 2);
        context.lineTo(x + offset, y + size / 2);
        context.closePath();
        context.stroke();
        break;
      }
      case "Square":
        context.strokeRect(x - size, y - size, size * 2, size * 2);
        break;
      case "Circle":
        context.arc(x, y, size, 0, Math.PI * 2);
        context.stroke();
        break;
      case "Cross":
        context.moveTo(x - size, y - size);
        context.lineTo(x + size, y + size);
        context.moveTo(x - size, y + size);
        context.lineTo(x + size, y - size);
        context.stroke();
        break;
      default: {
        const label = mark.text ?? "";
        if (!label) break;
        if (!stone) {
          context.fillStyle = wood(context, geometry, images);
          disc(context, x, y, Math.trunc(r * 0.72));
          context.fill();
        }
        context.fillStyle = color;
        text(context, label, x, y, UI_FONT, r * 1.4, r * 1.4);
      }
    }
  }
  context.restore();
}
