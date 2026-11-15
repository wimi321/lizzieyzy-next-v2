import { type ReactNode, type WheelEvent, useLayoutEffect, useRef, useState } from "react";
import { type Arrangement, DEFAULT_ARRANGEMENT, frameLayout } from "./arrange";
import type { FrameLayout, Rect } from "./layout";

export type RegionName = keyof FrameLayout;

const TEST_IDS: Partial<Record<RegionName, string>> = { board: "board-area" };
const NOMINAL = { width: 1280, height: 720 };

const DATA_NAMES: Record<RegionName, string> = {
  board: "board",
  captured: "captured",
  statistics: "statistics",
  graph: "graph",
  comment: "comment",
  status: "status",
  tree: "tree",
  list: "list",
  subBoard: "sub-board",
  extra: "extra",
};

/** A panel laid over some regions, as Java lays 存档与读档 over the list and the tree. */
export interface PanelCover {
  regions: readonly RegionName[];
  node: ReactNode;
}

export interface MainPanelProps {
  regions: Partial<Record<RegionName, ReactNode>>;
  /** Panels switched off under 显示 → 面板; the rest are laid out around them as Java does. */
  hidden?: readonly RegionName[];
  boardSize: number;
  /** Extra attributes on the board region, which tests use to match it to the document. */
  boardData?: Record<`data-${string}`, string | number | undefined>;
  onBoardWheel?(event: WheelEvent<HTMLElement>): void;
  /** Laid over the board region and kept to it, as 自由涂鸦's sheet. */
  boardOverlay?: ReactNode;
  cover?: PanelCover;
  /** 布局模式, the two enlarged layouts and the board's position. */
  arrangement?: Arrangement;
}

function style(rect: Rect) {
  return { left: rect.x, top: rect.y, width: rect.width, height: rect.height };
}

/** The smallest rectangle around the given ones; null when none has an area. */
function union(rects: readonly Rect[]): Rect | null {
  const real = rects.filter((rect) => rect.width > 0 && rect.height > 0);
  if (real.length === 0) return null;
  const x = Math.min(...real.map((rect) => rect.x));
  const y = Math.min(...real.map((rect) => rect.y));
  const right = Math.max(...real.map((rect) => rect.x + rect.width));
  const bottom = Math.max(...real.map((rect) => rect.y + rect.height));
  return { x, y, width: right - x, height: bottom - y };
}

/**
 * The window's content between the toolbars, cut into LizzieYzy Next's panels by its own
 * layout formula (`InFrameLayout`) every time the panel changes size.
 */
export function MainPanel({ regions, hidden = [], boardSize, boardData, onBoardWheel, boardOverlay, cover, arrangement = DEFAULT_ARRANGEMENT }: MainPanelProps) {
  const root = useRef<HTMLDivElement>(null);
  // A nominal panel until the first measurement, so the first frame is never empty.
  const [size, setSize] = useState(NOMINAL);

  useLayoutEffect(() => {
    const element = root.current;
    if (!element) return;
    const measure = () => {
      if (element.clientWidth > 0 && element.clientHeight > 0) setSize({ width: element.clientWidth, height: element.clientHeight });
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const shown = (name: RegionName) => !hidden.includes(name);
  const layout = frameLayout(
    size.width,
    size.height,
    boardSize,
    {
      comment: shown("comment"),
      subBoard: shown("subBoard"),
      tree: shown("tree"),
      status: shown("status"),
      list: regions.list !== undefined && shown("list"),
      graph: shown("graph"),
      info: shown("captured"),
    },
    arrangement,
  );
  // 思考模式 and 四方图 alone use the extra region.
  const present = (name: RegionName) => shown(name) && (name !== "extra" || (regions.extra != null && layout.extra.width > 0));
  // With neither region shown, the panel takes the right half, where they would be.
  const covered = cover ? (union(cover.regions.filter(shown).map((name) => layout[name])) ?? { x: size.width / 2, y: 0, width: size.width / 2, height: size.height }) : null;
  return (
    <div className="main-panel" ref={root} data-testid="main-panel">
      {(Object.keys(DATA_NAMES) as RegionName[]).filter(present).map((name) => (
        <section
          key={name}
          className="main-region"
          data-region={DATA_NAMES[name]}
          data-testid={TEST_IDS[name]}
          style={style(layout[name])}
          onWheel={name === "board" ? onBoardWheel : undefined}
          {...(name === "board" ? boardData : undefined)}
        >
          {regions[name]}
          {name === "board" ? boardOverlay : null}
        </section>
      ))}
      {cover && covered ? (
        <section className="main-cover" data-testid="main-cover" style={style(covered)}>
          {cover.node}
        </section>
      ) : null}
    </div>
  );
}
