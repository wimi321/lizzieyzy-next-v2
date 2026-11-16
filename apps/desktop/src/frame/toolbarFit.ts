import { type RefObject, useLayoutEffect, useState } from "react";

/** One child of a toolbar group: optional buttons carry their id, the rest always stay. */
export interface FitItem {
  id: string | null;
  width: number;
}

export interface FitRow {
  left: readonly FitItem[];
  center: readonly FitItem[];
  right: readonly FitItem[];
}

export interface FitOptions {
  /** The bar's width, its side padding and the gap between buttons and groups, in CSS pixels. */
  width: number;
  padding: number;
  gap: number;
  /** Optional ids, the one kept longest first; ids not listed give way before all listed ones. */
  priority: readonly string[];
  /** The id of 更多, which holds whatever does not fit. */
  more: string;
  /** 更多 stays even when everything fits (it also holds 试下, 终局点目, 纯网络). */
  moreAlways: boolean;
}

export interface Fit {
  /** Optional ids that give way at this width, 更多 included when it is not needed. */
  hidden: readonly string[];
  /** Where the navigation group starts: centred when there is room, moved aside otherwise. */
  start: number;
  centered: boolean;
  /** The left grid track that puts the navigation group at `start`. */
  leftTrack: number;
}

function span(items: readonly FitItem[], gap: number): number {
  return items.length === 0 ? 0 : items.reduce((total, item) => total + item.width, 0) + gap * (items.length - 1);
}

function attempt(row: FitRow, options: FitOptions, withMore: boolean): Fit {
  const { width, padding, gap, more } = options;
  const all = [...row.left, ...row.center, ...row.right];
  const shown = new Set(all.filter((item) => item.id === null || (item.id === more && withMore)));
  const sides = () => {
    const pick = (items: readonly FitItem[]) => items.filter((item) => shown.has(item));
    return { left: span(pick(row.left), gap), center: span(pick(row.center), gap), right: span(pick(row.right), gap) };
  };
  const fits = () => {
    const { left, center, right } = sides();
    return padding * 2 + left + (left > 0 ? gap : 0) + center + (right > 0 ? gap : 0) + right <= width;
  };
  // Java's BottomToolbar: what matters most is placed first; a wide button that does not fit
  // leaves its room to a narrower one after it.
  const optional = all.filter((item) => item.id !== null && item.id !== more);
  const rank = (item: FitItem) => {
    const index = options.priority.indexOf(item.id ?? "");
    return index < 0 ? options.priority.length + optional.indexOf(item) : index;
  };
  for (const item of [...optional].sort((a, b) => rank(a) - rank(b))) {
    shown.add(item);
    if (!fits()) shown.delete(item);
  }
  const { left, center, right } = sides();
  const minStart = padding + left + (left > 0 ? gap : 0);
  const maxStart = width - padding - right - (right > 0 ? gap : 0) - center;
  const centered = (width - center) / 2;
  const start = Math.max(minStart, Math.min(centered, maxStart));
  const hidden = all.filter((item) => item.id !== null && !shown.has(item)).map((item) => item.id as string);
  return { hidden, start, centered: start === centered, leftTrack: Math.max(0, start - padding - gap) };
}

/**
 * Which bottom toolbar buttons fit at this width (Java's `BottomToolbar` layout): the always
 * buttons stay, the optional ones are placed by priority, and 更多 appears for the rest.
 */
export function fitToolbar(row: FitRow, options: FitOptions): Fit {
  const first = attempt(row, options, options.moreAlways);
  if (options.moreAlways || first.hidden.every((id) => id === options.more)) return first;
  return attempt(row, options, true);
}

function same(a: Fit | null, b: Fit): boolean {
  return a !== null && a.centered === b.centered && Math.abs(a.start - b.start) < 0.5 && a.hidden.join() === b.hidden.join();
}

/** Measures the bar's three groups and fits them; null until the bar has a width (jsdom never). */
export function useToolbarFit(bar: RefObject<HTMLElement | null>, signature: string, priority: readonly string[], more: string, moreAlways: boolean): Fit | null {
  const [fit, setFit] = useState<Fit | null>(null);

  useLayoutEffect(() => {
    const element = bar.current;
    if (!element) return;
    const measure = () => {
      const rect = element.getBoundingClientRect();
      // Rectangles follow the interface's zoom; the style's pixels do not.
      const scale = element.offsetWidth > 0 ? rect.width / element.offsetWidth : 1;
      if (rect.width <= 0 || scale <= 0) return;
      const style = getComputedStyle(element);
      const groups = Array.from(element.children) as HTMLElement[];
      const items = (group: HTMLElement | undefined): FitItem[] =>
        Array.from(group?.children ?? []).map((child) => ({
          id: (child as HTMLElement).dataset.fit ?? null,
          width: child.getBoundingClientRect().width / scale,
        }));
      const next = fitToolbar(
        { left: items(groups[0]), center: items(groups[1]), right: items(groups[2]) },
        {
          width: rect.width / scale,
          padding: parseFloat(style.paddingLeft) || 0,
          gap: parseFloat(style.columnGap) || 0,
          priority,
          more,
          moreAlways,
        },
      );
      setFit((previous) => (same(previous, next) ? previous : next));
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    // The bar's width, a label that changes (自动分析's progress) or the interface's zoom.
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    for (const group of Array.from(element.children)) for (const child of Array.from(group.children)) observer.observe(child);
    return () => observer.disconnect();
  }, [bar, signature, priority, more, moreAlways]);

  return fit;
}
