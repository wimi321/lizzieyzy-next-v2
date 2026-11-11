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
  width: number;
  height: number;
  dpr: number;
}

/** An element's CSS size and the device pixel ratio, kept current as it is laid out again. */
export function useElementSize(element: RefObject<HTMLElement | null>): ElementSize {
  const [size, setSize] = useState<ElementSize>({ width: 0, height: 0, dpr: 1 });
  useEffect(() => {
    const target = element.current;
    if (!target || typeof ResizeObserver === "undefined") return;
    // ResizeObserver reports the initial size too, so no synchronous measuring here.
    const observer = new ResizeObserver(() => {
      const rect = target.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      setSize((previous) =>
        previous.width === rect.width && previous.height === rect.height && previous.dpr === dpr ? previous : { width: rect.width, height: rect.height, dpr },
      );
    });
    observer.observe(target);
    return () => observer.disconnect();
  }, [element]);
  return size;
}
