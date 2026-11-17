import { type MouseEvent, useCallback, useEffect, useRef, useState } from "react";
import type { Backend, CaptureCells, CaptureGrid, CapturePoint, CaptureRecognition } from "@/api/backend";
import type en from "../locales/en.json";
import { BOARD_SIZES, cellAt, cycled, defaultBoardSize, fits, gridLines, intersection, type Line, makesCells, pngSize, previewLines, type Size, toImage } from "./geometry";
import { describe, fill, LatestOnly } from "./values";

export type CaptureLabels = typeof en.capture;
export type CapturePageBackend = Pick<Backend, "captureImage" | "recognizeCapture" | "finishCapture" | "cancelCapture">;

/** Java's helper re-recognizes on every move; here at most every 50 ms, the latest pointer only. */
const PREVIEW_INTERVAL = 50;
/** While the pointer is this near the top (CSS pixels), the bar moves to the bottom out of its way. */
const BAR_ZONE = 160;

interface Confirming {
  grid: CaptureGrid;
  cells: CaptureCells;
  size: number | null;
}

export interface CapturePageProps {
  backend: CapturePageBackend;
  session: number;
  labels: CaptureLabels;
  /** The current game's size, which the main window left for this window. */
  preferredSize: number | null;
}

function Lines({ lines, className }: { lines: readonly Line[]; className: string }) {
  return (
    <g className={className}>
      {lines.map((line, index) => (
        <line key={index} {...line} vectorEffect="non-scaling-stroke" />
      ))}
    </g>
  );
}

/** Java's preview dots: yellow for black stones, magenta for white ones. */
function Dots({ grid, cells }: { grid: CaptureGrid; cells: CaptureCells }) {
  const radius = Math.min(grid.gap_x, grid.gap_y) / 3.5;
  return (
    <g data-testid="capture-dots">
      {cells.cells.map((cell, index) => {
        if (cell === "Empty") return null;
        const at = intersection(grid, Math.floor(index / cells.columns), index % cells.columns);
        return <circle key={index} className={cell === "Black" ? "capture-dot black" : "capture-dot white"} cx={at.x} cy={at.y} r={radius} data-cell={cell} />;
      })}
    </g>
  );
}

/**
 * The capture window (`?window=capture&session=`): the frozen screenshot, Java's three clicks with
 * its live preview, then the result to correct point by point and place on a board.
 */
export function CapturePage({ backend, session, labels, preferredSize }: CapturePageProps) {
  const text = labels.page;
  const [url, setUrl] = useState<string | null>(null);
  const [image, setImage] = useState<Size | null>(null);
  const [points, setPoints] = useState<CapturePoint[]>([]);
  const [pointer, setPointer] = useState<CapturePoint | null>(null);
  const [preview, setPreview] = useState<CaptureRecognition | null>(null);
  const [confirming, setConfirming] = useState<Confirming | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [low, setLow] = useState(false);
  const area = useRef<HTMLDivElement>(null);
  // The live preview: only the newest pointer is recognized. The third click is sent on its own.
  const [previews] = useState(
    () => new LatestOnly<[CapturePoint, CapturePoint, CapturePoint], CaptureRecognition>((points) => backend.recognizeCapture?.(session, points) ?? Promise.reject(new Error("unavailable")), setPreview, PREVIEW_INTERVAL),
  );
  const sequence = useRef(0);

  useEffect(() => {
    let active = true;
    let made: string | null = null;
    backend
      .captureImage?.(session)
      .then((bytes) => {
        if (!active) return;
        setImage(pngSize(bytes));
        made = URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
        setUrl(made);
      })
      .catch((reason: unknown) => active && setError(fill(labels.failed, { reason: describe(reason) })));
    return () => {
      active = false;
      previews.drop();
      if (made) URL.revokeObjectURL(made);
    };
  }, [backend, session, labels.failed, previews]);

  const cancel = useCallback(() => {
    sequence.current++;
    previews.drop();
    void backend.cancelCapture?.(session).catch(() => undefined);
  }, [backend, session, previews]);

  const undo = useCallback(() => {
    sequence.current++;
    previews.drop();
    setError(null);
    if (confirming) {
      setConfirming(null);
      setPoints((previous) => previous.slice(0, 2));
      return;
    }
    setPreview(null);
    setPoints((previous) => previous.slice(0, -1));
  }, [confirming, previews]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        cancel();
      } else if (event.key === "Backspace") {
        event.preventDefault();
        undo();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [cancel, undo]);

  const pointAt = (event: MouseEvent<HTMLElement>): CapturePoint | null => {
    const box = area.current?.getBoundingClientRect();
    return box && image ? toImage(event.clientX, event.clientY, box, image) : null;
  };

  const onMove = (event: MouseEvent<HTMLDivElement>) => {
    const box = area.current?.getBoundingClientRect();
    if (box) setLow(event.clientY - box.top < BAR_ZONE);
    const at = pointAt(event);
    if (!at || confirming) return;
    setPointer(at);
    const [first, second] = points;
    if (points.length === 2 && first && second) previews.push([first, second, at]);
  };

  const onClick = (event: MouseEvent<HTMLDivElement>) => {
    const at = pointAt(event);
    if (!at || busy) return;
    setError(null);
    if (confirming) {
      const cell = cellAt(confirming.grid, at);
      if (cell) setConfirming({ ...confirming, cells: cycled(confirming.cells, cell.row, cell.column) });
      return;
    }
    const [first, second] = points;
    if (!first) {
      setPoints([at]);
      return;
    }
    if (!second) {
      if (makesCells(first, at)) setPoints([first, at]);
      else setError(text.gridError);
      return;
    }
    // The third click: recognize these exact points, then let the result be corrected.
    const mine = ++sequence.current;
    previews.drop();
    setBusy(true);
    backend
      .recognizeCapture?.(session, [first, second, at])
      .then((result) => {
        if (mine !== sequence.current) return;
        setPoints([first, second, at]);
        setPreview(result);
        setConfirming({ grid: result.grid, cells: result.cells, size: defaultBoardSize(result.cells, preferredSize) });
      })
      .catch(() => mine === sequence.current && setError(text.gridError))
      .finally(() => setBusy(false));
  };

  const finish = () => {
    if (!confirming || confirming.size === null) return;
    setBusy(true);
    setError(null);
    backend
      .finishCapture?.(session, { corner: confirming.grid.corner, cells: confirming.cells, board_size: confirming.size })
      .catch((reason: unknown) => {
        setBusy(false);
        setError(fill(text.finishFailed, { reason: describe(reason) }));
      });
  };

  const again = () => {
    sequence.current++;
    previews.drop();
    setConfirming(null);
    setPreview(null);
    setPoints([]);
    setError(null);
  };

  const [first, second] = points;
  const stage = confirming ? "confirm" : points.length;
  // While correcting, the bar sits in the half of the screen away from the problem.
  const lastPoint = confirming ? intersection(confirming.grid, confirming.grid.rows - 1, confirming.grid.columns - 1) : null;
  const barLow = confirming && lastPoint && image ? (confirming.grid.origin.y + lastPoint.y) / 2 < image.height / 2 : low;
  const hint = confirming ? text.fix : [labels.step1, labels.step2, labels.step3][points.length] ?? "";
  const shown = confirming ?? (points.length === 2 ? preview : null);

  return (
    <div className="capture-page" data-testid="capture-page" data-stage={stage}>
      <div className={barLow ? "capture-bar low" : "capture-bar"} role="toolbar" aria-label={labels.title} data-testid="capture-bar" onClick={(event) => event.stopPropagation()}>
        <p className="capture-hint" data-testid="capture-hint">
          {url ? hint : text.loading}
        </p>
        {confirming ? (
          <div className="capture-confirm" data-testid="capture-confirm">
            <span data-testid="capture-size">
              {fill(text.size, { columns: confirming.cells.columns, rows: confirming.cells.rows })} · {text.corners[confirming.grid.corner]}
            </span>
            <span className="capture-sizes" role="radiogroup" aria-label={text.boardSize}>
              {text.boardSize}
              {BOARD_SIZES.map((size) => (
                <button
                  key={size}
                  type="button"
                  role="radio"
                  aria-checked={confirming.size === size}
                  disabled={!fits(confirming.cells, size)}
                  data-testid={`capture-board-${size}`}
                  onClick={() => setConfirming({ ...confirming, size })}
                >
                  {fill(text.boardSizeOption, { n: size })}
                </button>
              ))}
            </span>
            <button type="button" className="secondary-button" data-testid="capture-again" onClick={again}>
              {text.again}
            </button>
            <button type="button" className="secondary-button" data-testid="capture-cancel" onClick={cancel}>
              {text.cancel}
            </button>
            <button type="button" className="primary-button" data-testid="capture-finish" disabled={busy || confirming.size === null} onClick={finish}>
              {text.confirm}
            </button>
          </div>
        ) : (
          <span className="capture-keys">{text.keys}</span>
        )}
        {confirming && confirming.size === null ? (
          <p className="capture-error" role="alert" data-testid="capture-too-large">
            {text.tooLarge}
          </p>
        ) : null}
        {error ? (
          <p className="capture-error" role="alert" data-testid="capture-error">
            {error}
          </p>
        ) : null}
      </div>
      <div className="capture-area" ref={area} data-testid="capture-area" onMouseMove={onMove} onClick={onClick}>
        {url ? <img className="capture-image" src={url} alt={text.label} draggable={false} /> : null}
        {image ? (
          <svg className="capture-overlay" viewBox={`0 0 ${image.width} ${image.height}`} preserveAspectRatio="none" aria-hidden="true">
            <rect className="capture-tint" x={0} y={0} width={image.width} height={image.height} />
            {!confirming && pointer && points.length < 2 ? (
              <Lines className="capture-crosshair" lines={[{ x1: pointer.x, y1: 0, x2: pointer.x, y2: image.height }, { x1: 0, y1: pointer.y, x2: image.width, y2: pointer.y }]} />
            ) : null}
            {!confirming && first && pointer && points.length === 1 ? <Lines className="capture-preview" lines={previewLines(first, pointer)} /> : null}
            {!confirming && first && second && pointer ? (
              <rect
                className="capture-box"
                x={Math.min(first.x, pointer.x)}
                y={Math.min(first.y, pointer.y)}
                width={Math.abs(pointer.x - first.x)}
                height={Math.abs(pointer.y - first.y)}
                vectorEffect="non-scaling-stroke"
              />
            ) : null}
            {confirming && lastPoint ? (
              <rect
                className="capture-box"
                x={confirming.grid.origin.x}
                y={confirming.grid.origin.y}
                width={lastPoint.x - confirming.grid.origin.x}
                height={lastPoint.y - confirming.grid.origin.y}
                vectorEffect="non-scaling-stroke"
              />
            ) : null}
            {shown ? (
              <>
                <Lines className="capture-grid" lines={gridLines(shown.grid)} />
                <Dots grid={shown.grid} cells={shown.cells} />
              </>
            ) : null}
          </svg>
        ) : null}
      </div>
    </div>
  );
}
