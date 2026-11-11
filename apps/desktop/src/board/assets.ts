import { type RefObject, useEffect, useState } from "react";
import blackUrl from "../assets/java/black0.png?no-inline";
import boardUrl from "../assets/java/board.png?no-inline";
import whiteUrl from "../assets/java/white0.png?no-inline";
import { type BoardImages, NUMBER_FONT, settleFonts, UI_FONT } from "./draw";

function load(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = reject;
    image.src = url;
  });
}

let imagesPromise: Promise<BoardImages> | null = null;
function loadImages(): Promise<BoardImages> {
  imagesPromise ??= Promise.all([load(boardUrl), load(blackUrl), load(whiteUrl)]).then(([board, black, white]) => ({ board, black, white }));
  return imagesPromise;
}

/**
 * The wood and stone images and the board's fonts, shared by every board on screen.
 * `fonts` counts up once the fonts are in, so that a canvas drawn with a fallback redraws.
 */
export function useBoardAssets(): { images: BoardImages | null; fonts: number } {
  const [images, setImages] = useState<BoardImages | null>(null);
  const [fonts, setFonts] = useState(0);
  useEffect(() => {
    let active = true;
    loadImages()
      .then((loaded) => active && setImages(loaded))
      .catch((error: unknown) => console.error("Board images failed to load", error));
    const fontSet = typeof document !== "undefined" ? document.fonts : undefined;
    if (!fontSet?.load) return;
    // Canvas text does not trigger a font download by itself.
    Promise.allSettled([fontSet.load(`16px ${NUMBER_FONT}`), fontSet.load(`16px ${UI_FONT}`)]).then(() => {
      settleFonts();
      if (active) setFonts((count) => count + 1);
    });
    return () => {
      active = false;
    };
  }, []);
  return { images, fonts };
}

export interface ElementSize {
  /** CSS pixels. */
  width: number;
  height: number;
  /** The same box in device pixels: the size a canvas needs for one pixel per device pixel. */
  deviceWidth: number;
  deviceHeight: number;
  dpr: number;
}

/**
 * An element's length in device pixels. At 125% or 150% a length can fall on half a device
 * pixel, and only the browser knows which way it rounded: a canvas one pixel off is resampled
 * and every line on it blurs. So the browser's own count is taken when it agrees with the scale.
 * WebKit gives none and an emulated scale reports CSS pixels there; then the length is scaled
 * and rounded, which is exact at the whole-number scales those run at.
 */
export function devicePixels(exact: number | undefined, css: number, dpr: number): number {
  const scaled = css * dpr;
  return exact !== undefined && Math.abs(exact - scaled) < 1 ? exact : Math.round(scaled);
}

const UNMEASURED: ElementSize = { width: 0, height: 0, deviceWidth: 0, deviceHeight: 0, dpr: 1 };

/** An element's size in CSS and in device pixels, kept current as it is laid out again. */
export function useElementSize(element: RefObject<HTMLElement | null>): ElementSize {
  const [size, setSize] = useState(UNMEASURED);
  useEffect(() => {
    const target = element.current;
    if (!target || typeof ResizeObserver === "undefined") return;
    // ResizeObserver reports the initial size too, so no synchronous measuring here.
    const observer = new ResizeObserver((entries) => {
      const rect = target.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      // Absent in WebKit, whatever the type says.
      const exact = (entries[entries.length - 1] as Partial<ResizeObserverEntry> | undefined)?.devicePixelContentBoxSize?.[0];
      const next: ElementSize = {
        width: rect.width,
        height: rect.height,
        deviceWidth: devicePixels(exact?.inlineSize, rect.width, dpr),
        deviceHeight: devicePixels(exact?.blockSize, rect.height, dpr),
        dpr,
      };
      setSize((previous) =>
        previous.width === next.width &&
        previous.height === next.height &&
        previous.deviceWidth === next.deviceWidth &&
        previous.deviceHeight === next.deviceHeight &&
        previous.dpr === next.dpr
          ? previous
          : next,
      );
    });
    try {
      observer.observe(target, { box: "device-pixel-content-box" });
    } catch {
      // WebKit rejects the box it does not know.
      observer.observe(target);
    }
    return () => observer.disconnect();
  }, [element]);
  return size;
}
