/**
 * 保存/复制 棋盘截图、保存胜率图截图: PNG bytes of what is on screen. The board and the
 * sub-board are stacks of canvases in device pixels; the winrate graph is an SVG styled by CSS.
 */

function encode(canvas: HTMLCanvasElement): Promise<Uint8Array> {
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (!blob) return reject(new Error("PNG encoding failed"));
      blob.arrayBuffer().then((buffer) => resolve(new Uint8Array(buffer)), reject);
    }, "image/png");
  });
}

function element(testId: string): Element {
  const found = document.querySelector(`[data-testid="${testId}"]`);
  if (!found) throw new Error(`Nothing on screen to capture: ${testId}`);
  return found;
}

/** The board's layers (wood and grid, stones, analysis) flattened into one picture. */
export function canvasPng(testId: string): Promise<Uint8Array> {
  const layers = Array.from(element(testId).querySelectorAll("canvas"));
  const base = layers[0];
  if (!base || base.width === 0 || base.height === 0) return Promise.reject(new Error(`Nothing drawn yet: ${testId}`));
  const out = document.createElement("canvas");
  out.width = base.width;
  out.height = base.height;
  const context = out.getContext("2d");
  if (!context) return Promise.reject(new Error("Canvas is unavailable"));
  for (const layer of layers) context.drawImage(layer, 0, 0, out.width, out.height);
  return encode(out);
}

/** What CSS gives the graph's shapes and text; an SVG drawn as an image sees no style sheet. */
const STYLE = [
  "fill",
  "fill-opacity",
  "stroke",
  "stroke-width",
  "stroke-opacity",
  "stroke-dasharray",
  "stroke-linecap",
  "stroke-linejoin",
  "opacity",
  "visibility",
  "display",
  "font-family",
  "font-size",
  "font-weight",
  "text-anchor",
  "dominant-baseline",
  "paint-order",
];

function background(start: Element): string {
  for (let node: Element | null = start; node; node = node.parentElement) {
    const color = getComputedStyle(node).backgroundColor;
    if (color && color !== "transparent" && !/rgba\(.*,\s*0\)$/.test(color)) return color;
  }
  return "#ffffff";
}

export async function svgPng(testId: string): Promise<Uint8Array> {
  const container = element(testId);
  const svg = container.querySelector("svg");
  if (!svg) throw new Error(`Nothing drawn yet: ${testId}`);
  const width = svg.width.baseVal.value;
  const height = svg.height.baseVal.value;
  if (width <= 0 || height <= 0) throw new Error(`Nothing drawn yet: ${testId}`);
  const copy = svg.cloneNode(true) as SVGSVGElement;
  const source = [svg, ...Array.from(svg.querySelectorAll("*"))];
  const target = [copy, ...Array.from(copy.querySelectorAll("*"))];
  source.forEach((node, index) => {
    const style = getComputedStyle(node);
    target[index]?.setAttribute("style", STYLE.map((name) => `${name}:${style.getPropertyValue(name)}`).join(";"));
  });
  copy.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  const image = new Image();
  image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(new XMLSerializer().serializeToString(copy))}`;
  await image.decode();
  const scale = window.devicePixelRatio || 1;
  const out = document.createElement("canvas");
  out.width = Math.round(width * scale);
  out.height = Math.round(height * scale);
  const context = out.getContext("2d");
  if (!context) throw new Error("Canvas is unavailable");
  context.fillStyle = background(container);
  context.fillRect(0, 0, out.width, out.height);
  context.drawImage(image, 0, 0, out.width, out.height);
  return encode(out);
}
