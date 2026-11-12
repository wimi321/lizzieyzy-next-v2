import type { Geometry } from "./geometry";
import { fitFontSize } from "./suggestion";

/** The canvas basics every board layer shares: fonts, sizing, discs, the wood and Java's text. */

/** Java's logical `Dialog` font: Lucida Grande on macOS, Arial on Windows, DejaVu on Linux. */
export const UI_FONT = '"Lucida Grande", Arial, "DejaVu Sans", "PingFang SC", "Microsoft YaHei", sans-serif';
/** Open Sans Semibold, which Java ships for winrates and scores. */
export const NUMBER_FONT = `"Lizzie Open Sans", ${UI_FONT}`;

export interface BoardImages {
  board: CanvasImageSource;
  black: CanvasImageSource;
  white: CanvasImageSource;
}

export type Context = CanvasRenderingContext2D;
const TAU = Math.PI * 2;

/** Sizes the backing store to the geometry and clears it. */
export function prepare(canvas: HTMLCanvasElement, geometry: Geometry): Context | null {
  // Nothing to draw before the first layout (and never in jsdom, which has no canvas).
  if (geometry.cell <= 0) return null;
  if (canvas.width !== geometry.width) canvas.width = geometry.width;
  if (canvas.height !== geometry.height) canvas.height = geometry.height;
  const context = canvas.getContext("2d");
  if (!context) return null;
  context.setTransform(1, 0, 0, 1, 0, 0);
  context.clearRect(0, 0, canvas.width, canvas.height);
  return context;
}

/** Java's `fillOval(x - r, y - r, 2r + 1, 2r + 1)`: a disc of radius r + ½ about the pixel centre. */
export function disc(context: Context, x: number, y: number, radius: number) {
  context.beginPath();
  context.arc(x, y, radius + 0.5, 0, TAU);
}

export function wood(context: Context, geometry: Geometry, images: BoardImages): CanvasPattern | string {
  const pattern = context.createPattern(images.board, "repeat");
  if (!pattern) return "#dbb973";
  pattern.setTransform(new DOMMatrix().translate(geometry.boardX, geometry.boardY));
  return pattern;
}

// ---------------------------------------------------------------- text

const widths = new Map<string, number>();

function widthAt100(context: Context, family: string, text: string): number {
  const key = `${family}\n${text}`;
  let width = widths.get(key);
  if (width === undefined) {
    context.font = `100px ${family}`;
    width = context.measureText(text).width;
    // A font that is still loading measures with its fallback; do not remember that.
    if (widths.size > 4000) widths.clear();
    if (fontsSettled) widths.set(key, width);
  }
  return width;
}

let fontsSettled = false;
/** Called once the web fonts are in, so cached measurements are the real ones. */
export function settleFonts() {
  widths.clear();
  fontsSettled = true;
}

export interface TextBox {
  left: number;
  baseline: number;
  width: number;
  /** Java's `ascent - descent`, the height it centres text by. */
  height: number;
}

/**
 * `BoardRenderer.drawString`: the largest font within the limits, centred on x.
 * `anchor` says where y is: the text's middle, its baseline ("above", Java's aboveOrBelow 1)
 * or its top ("below").
 */
export function text(
  context: Context,
  string: string,
  x: number,
  y: number,
  family: string,
  maximumHeight: number,
  maximumWidth: number,
  anchor: "middle" | "above" | "below" = "middle",
  paint = true,
): TextBox {
  const size = fitFontSize(widthAt100(context, family, string), maximumHeight, maximumWidth);
  context.font = `${size}px ${family}`;
  context.textAlign = "left";
  context.textBaseline = "alphabetic";
  const metrics = context.measureText(string);
  const height = roundUp(metrics.fontBoundingBoxAscent) - roundUp(metrics.fontBoundingBoxDescent);
  const half = Math.trunc(height / 2);
  const width = Math.round(metrics.width);
  // Java places text on whole pixels; x and y arrive as pixel centres or as whole pixels.
  const baseline = Math.floor(y) + half + (anchor === "below" ? half : anchor === "above" ? -half : 0);
  const left = Math.floor(x) - Math.trunc(width / 2);
  if (paint) context.fillText(string, left, baseline);
  return { left, baseline, width, height };
}

/** `FontMetrics` rounds an ascent or descent up unless it is within 0.05 of the pixel below. */
const roundUp = (value: number) => Math.floor(value + 0.95);
