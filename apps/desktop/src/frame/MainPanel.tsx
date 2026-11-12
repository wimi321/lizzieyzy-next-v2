import { type ReactNode, type WheelEvent, useLayoutEffect, useRef, useState } from "react";
import { type FrameLayout, inFrameLayout, type Rect } from "./layout";

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
  subBoard: "sub-board",
};

export interface MainPanelProps {
  regions: Partial<Record<RegionName, ReactNode>>;
  /** Panels switched off under 显示 → 面板; the rest are laid out around them as Java does. */
  hidden?: readonly RegionName[];
  boardSize: number;
  /** Extra attributes on the board region, which tests use to match it to the document. */
  boardData?: Record<`data-${string}`, string | number | undefined>;
  onBoardWheel?(event: WheelEvent<HTMLElement>): void;
}

function style(rect: Rect) {
  return { left: rect.x, top: rect.y, width: rect.width, height: rect.height };
}

/**
 * The window's content between the toolbars, cut into LizzieYzy Next's panels by its own
 * layout formula (`InFrameLayout`) every time the panel changes size.
 */
export function MainPanel({ regions, hidden = [], boardSize, boardData, onBoardWheel }: MainPanelProps) {
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
  const layout = inFrameLayout(size.width, size.height, boardSize, {
    comment: shown("comment"),
    subBoard: shown("subBoard"),
    tree: shown("tree"),
    status: shown("status"),
  });
  return (
    <div className="main-panel" ref={root} data-testid="main-panel">
      {(Object.keys(DATA_NAMES) as RegionName[]).filter(shown).map((name) => (
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
        </section>
      ))}
    </div>
  );
}
