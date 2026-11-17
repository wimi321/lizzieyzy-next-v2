import { describe, expect, test } from "vitest";
import type { LayoutMode } from "@/api/backend";
import { type Arrangement, DEFAULT_ARRANGEMENT, frameLayout } from "./arrange";
import { ALL_PANELS, type FrameLayout, inFrameLayout, type PanelFlags, type Rect } from "./layout";

const EVERYTHING: PanelFlags = { ...ALL_PANELS, list: true, graph: true, info: true };
const SIZES: [number, number][] = [
  [1429, 833],
  [1280, 720],
  [1920, 963],
  [1080, 680],
  [800, 700],
  [700, 700],
  [520, 400],
  [700, 1000],
];
const MODES: LayoutMode[] = ["Normal", "Min", "Thinking", "MultiView", "DoubleEngine"];

const arrangement = (change: Partial<Arrangement>): Arrangement => ({ ...DEFAULT_ARRANGEMENT, ...change });
const shown = (layout: FrameLayout) => (Object.entries(layout) as [keyof FrameLayout, Rect][]).filter(([, area]) => area.width > 0 && area.height > 0);

function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/** Whether MainPanel draws the region: the panel is switched on (the board and the square always). */
function visible(name: keyof FrameLayout, flags: PanelFlags): boolean {
  if (name === "captured" || name === "statistics") return flags.info !== false;
  if (name === "board" || name === "extra" || name === "lines") return true;
  return flags[name] !== false && flags[name] !== undefined;
}

/** Every panel that shows lies inside the main panel and covers no other. */
function expectTidy(layout: FrameLayout, flags: PanelFlags, width: number, height: number, label: string) {
  const areas = shown(layout).filter(([name]) => visible(name, flags));
  for (const [name, area] of areas) {
    expect(area.x, `${label} ${name} x`).toBeGreaterThanOrEqual(0);
    expect(area.y, `${label} ${name} y`).toBeGreaterThanOrEqual(0);
    expect(area.x + area.width, `${label} ${name} right`).toBeLessThanOrEqual(width);
    expect(area.y + area.height, `${label} ${name} bottom`).toBeLessThanOrEqual(height);
  }
  for (let i = 0; i < areas.length; i++) {
    for (let j = i + 1; j < areas.length; j++) {
      const [a, first] = areas[i]!;
      const [b, second] = areas[j]!;
      expect(overlaps(first, second), `${label}: ${a} overlaps ${b}`).toBe(false);
    }
  }
}

describe("the main panel's layouts", () => {
  test("every mode and enlargement keeps the panels inside the window and apart, whichever panels show", () => {
    const flagSets: PanelFlags[] = [
      EVERYTHING,
      { ...EVERYTHING, comment: false },
      { ...EVERYTHING, tree: false, list: false },
      { ...EVERYTHING, info: false },
      { ...EVERYTHING, info: false, tree: false, comment: false, list: false },
      { ...EVERYTHING, status: false, graph: false },
    ];
    for (const [width, height] of SIZES) {
      for (const flags of flagSets) {
        for (const mode of MODES) {
          for (const large of [{}, { largeSubBoard: true }, { largeGraph: true }, { boardFloated: true }]) {
            const layout = frameLayout(width, height, 19, flags, arrangement({ mode, ...large }));
            expectTidy(layout, flags, width, height, `${width}×${height} ${mode} ${JSON.stringify(large)} ${JSON.stringify(flags)}`);
          }
        }
      }
    }
  });

  test("the default layout is Java's InFrameLayout, moved by 主棋盘/界面位置", () => {
    expect(frameLayout(1429, 833, 19, ALL_PANELS)).toEqual(inFrameLayout(1429, 833, 19, ALL_PANELS));
    const at = (position: number) => frameLayout(1429, 833, 19, ALL_PANELS, arrangement({ position })).board.x;
    expect(at(0)).toBe(0);
    expect(at(4)).toBe(296);
    expect(at(8)).toBe(592);
    expect(at(12)).toBe(592);
    expect(at(0)).toBeLessThan(at(1));
  });

  test("精简模式 leaves the board alone", () => {
    const layout = frameLayout(1429, 833, 19, EVERYTHING, arrangement({ mode: "Min" }));
    expect(shown(layout).map(([name]) => name)).toEqual(["board"]);
    expect(layout.board).toEqual({ x: 296, y: 0, width: 833, height: 833 });
  });

  test("放大小棋盘: the board on the right, a large sub-board at the bottom left, the list beside the graph", () => {
    const layout = frameLayout(1429, 833, 19, EVERYTHING, arrangement({ largeSubBoard: true }));
    expect(layout.board.x + layout.board.width).toBe(1429 - 16);
    expect(layout.subBoard.width).toBeGreaterThan(inFrameLayout(1429, 833).subBoard.width);
    expect(layout.subBoard.y + layout.subBoard.height).toBe(layout.status.y);
    expect(layout.list.y).toBe(layout.graph.y);
    expect(layout.list.x).toBe(layout.graph.x + layout.graph.width);
    expect(layout.tree.y + layout.tree.height).toBe(layout.comment.y);
    // Without the sub-board shown, it is the default layout.
    expect(frameLayout(1429, 833, 19, { ...EVERYTHING, subBoard: false }, arrangement({ largeSubBoard: true }))).toEqual(
      frameLayout(1429, 833, 19, { ...EVERYTHING, subBoard: false }),
    );
  });

  test("放大胜率图: the sub-board top left, a tall graph under the info box, no list", () => {
    const layout = frameLayout(1429, 833, 19, EVERYTHING, arrangement({ largeGraph: true }));
    expect(layout.subBoard.y).toBe(0);
    expect(layout.graph.height).toBeGreaterThan(layout.board.height / 2);
    expect(layout.graph.y).toBeGreaterThan(layout.statistics.y);
    expect(layout.list.width).toBe(0);
  });

  test("an upright window ignores the enlargements, as Java does, and so does one with no room beside the board", () => {
    const flags = EVERYTHING;
    for (const [width, height] of [[700, 1000], [700, 700]] as const) {
      for (const large of [{ largeSubBoard: true }, { largeGraph: true }]) {
        expect(frameLayout(width, height, 19, flags, arrangement(large))).toEqual(frameLayout(width, height, 19, flags));
      }
    }
  });

  test("思考模式 and 四方图: a square on the left as tall as the panel, the board in the room left over", () => {
    const wide = frameLayout(1429, 833, 19, EVERYTHING, arrangement({ mode: "Thinking" }));
    expect(wide.extra).toEqual({ x: 0, y: 0, width: 833, height: 833 });
    expect(wide.subBoard.width).toBe(0);
    expect(wide.board.x + wide.board.width).toBe(1429);
    const window = frameLayout(2200, 833, 19, EVERYTHING, arrangement({ mode: "MultiView" }));
    // Room enough beside the square: the panels sit in a column between it and the board.
    expect(window.graph.x).toBe(833);
    expect(window.tree.x).toBe(833);
    expect(window.board.x).toBeGreaterThan(window.graph.x + window.graph.width - 1);
    // The list keeps the comment out, as in Java.
    expect(window.comment.width).toBe(0);
    // An upright window puts the square on top.
    const upright = frameLayout(700, 1000, 19, EVERYTHING, arrangement({ mode: "Thinking" }));
    expect(upright.extra.y).toBe(0);
    expect(upright.board.y).toBeGreaterThanOrEqual(upright.extra.height);
  });

  test("浮动主棋盘: no board here; the info box, graph and comment on the left, the tree, list and sub-board on the right", () => {
    const layout = frameLayout(1000, 520, 19, EVERYTHING, arrangement({ boardFloated: true }));
    expect(layout.board.width).toBe(0);
    expect(layout.captured.x).toBe(0);
    expect(layout.graph.width).toBe(500);
    expect(layout.tree.x).toBe(500);
    expect(layout.subBoard.y + layout.subBoard.height).toBe(520);
    expect(layout.status.y + layout.status.height).toBe(520);
    // The position sets the left column's width; with nothing on the right it takes all of it.
    expect(frameLayout(1000, 520, 19, EVERYTHING, arrangement({ boardFloated: true, position: 2 })).graph.width).toBe(250);
    const alone = frameLayout(1000, 520, 19, { ...EVERYTHING, tree: false, list: false, subBoard: false }, arrangement({ boardFloated: true }));
    expect(alone.graph.width).toBe(1000);
  });
});
