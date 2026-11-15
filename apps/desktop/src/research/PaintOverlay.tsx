import { type PointerEvent, useEffect, useRef, useState } from "react";
import type { PaintingColor } from "@/api/backend";
import type en from "../locales/en.json";

type Labels = typeof en.research.paint;

/** Java's pens (`Color.BLUE`, `GREEN`, `RED`), 3 px wide. */
const PEN: Record<PaintingColor, string> = { Blue: "#0000ff", Green: "#00ff00", Red: "#ff0000" };
const COLORS: readonly PaintingColor[] = ["Blue", "Green", "Red"];
const WIDTH = 3;

interface Stroke {
  color: PaintingColor;
  /** In board units: the board's square is 0..1 both ways, so a drawing follows it when the window changes. */
  points: [number, number][];
}

/** Where a pointer is, in the coordinates of the square the board is drawn in (centred, as wide as it can be). */
function boardPoint(element: Element, event: PointerEvent): [number, number] {
  const rect = element.getBoundingClientRect();
  const size = Math.min(rect.width, rect.height) || 1;
  return [(event.clientX - rect.left - (rect.width - size) / 2) / size, (event.clientY - rect.top - (rect.height - size) / 2) / size];
}

function polyline(points: readonly [number, number][]): string {
  // A click is a dot: a line that ends where it starts, drawn with round caps.
  const shown = points.length === 1 ? [...points, ...points] : points;
  return shown.map(([x, y]) => `${x},${y}`).join(" ");
}

export interface PaintOverlayProps {
  color: PaintingColor;
  labels: Labels;
  onColor(color: PaintingColor): void;
  onClose(): void;
}

/**
 * 自由涂鸦: a clear sheet over the board to draw on with the left button. Nothing is kept: closing
 * it (Esc or ✕) throws the drawing away; only the pen's colour is remembered.
 */
export function PaintOverlay({ color, labels, onColor, onClose }: PaintOverlayProps) {
  const [strokes, setStrokes] = useState<Stroke[]>([]);
  const [drawing, setDrawing] = useState<Stroke | null>(null);
  const latest = useRef(onClose);
  useEffect(() => {
    latest.current = onClose;
  });

  // Esc closes the sheet before anything under it (the markup tools) sees the key.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      latest.current();
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);

  const start = (event: PointerEvent<SVGSVGElement>) => {
    // Only the left button draws; Java let a right drag join two strokes.
    if (event.button !== 0) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    setDrawing({ color, points: [boardPoint(event.currentTarget, event)] });
  };
  const move = (event: PointerEvent<SVGSVGElement>) => {
    if (!drawing || (event.buttons & 1) === 0) return;
    const point = boardPoint(event.currentTarget, event);
    setDrawing({ ...drawing, points: [...drawing.points, point] });
  };
  const end = (event: PointerEvent<SVGSVGElement>) => {
    if (event.button !== 0 || !drawing) return;
    setStrokes([...strokes, drawing]);
    setDrawing(null);
  };

  const shown = drawing ? [...strokes, drawing] : strokes;
  return (
    <div className="paint-overlay" data-testid="paint-overlay" data-strokes={strokes.length}>
      <svg
        className="paint-sheet"
        viewBox="0 0 1 1"
        preserveAspectRatio="xMidYMid meet"
        overflow="visible"
        aria-label={labels.title}
        role="img"
        onPointerDown={start}
        onPointerMove={move}
        onPointerUp={end}
        onPointerCancel={() => setDrawing(null)}
        onContextMenu={(event) => event.preventDefault()}
      >
        {shown.map((stroke, index) => (
          <polyline
            key={index}
            points={polyline(stroke.points)}
            fill="none"
            stroke={PEN[stroke.color]}
            strokeWidth={WIDTH}
            strokeLinecap="round"
            strokeLinejoin="round"
            vectorEffect="non-scaling-stroke"
          />
        ))}
      </svg>
      <div className="paint-tools" role="toolbar" aria-label={labels.title}>
        {COLORS.map((pen) => (
          <button
            key={pen}
            type="button"
            className="paint-pen"
            style={{ background: PEN[pen] }}
            aria-label={labels[pen]}
            title={labels[pen]}
            aria-pressed={pen === color}
            data-testid={`paint-${pen.toLowerCase()}`}
            onClick={() => onColor(pen)}
          />
        ))}
        <button type="button" className="secondary-button" disabled={strokes.length === 0} data-testid="paint-undo" onClick={() => setStrokes(strokes.slice(0, -1))}>
          {labels.undo}
        </button>
        <button type="button" className="secondary-button" disabled={strokes.length === 0} data-testid="paint-clear" onClick={() => setStrokes([])}>
          {labels.clear}
        </button>
        <button type="button" className="secondary-button" title="ESC" aria-label={labels.close} data-testid="paint-close" onClick={onClose}>
          ✕
        </button>
      </div>
    </div>
  );
}
