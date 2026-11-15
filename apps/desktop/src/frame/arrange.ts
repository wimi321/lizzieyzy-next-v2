import type { LayoutMode } from "@/api/backend";
import { BOARD_POSITION, type FrameLayout, inFrameLayout, type PanelFlags, type Rect, STATUS_HEIGHT } from "./layout";

/** 显示 → 布局模式, 主界面设置 and 主棋盘/界面位置: what decides the layout besides the panels. */
export interface Arrangement {
  mode: LayoutMode;
  largeSubBoard: boolean;
  largeGraph: boolean;
  /** Java's `BoardPositionProportion`, 0 (left) to 8 (right). */
  position: number;
  /** 浮动主棋盘 without 保留原棋盘: the board is in its own window, the panels share this one. */
  boardFloated?: boolean;
}

export const DEFAULT_ARRANGEMENT: Arrangement = { mode: "Normal", largeSubBoard: false, largeGraph: false, position: BOARD_POSITION };

const div = (a: number, b: number) => Math.trunc(a / b);
const rect = (x: number, y: number, width: number, height: number): Rect => ({ x, y, width: Math.max(0, width), height: Math.max(0, height) });
const NONE: Rect = rect(0, 0, 0, 0);

function empty(): FrameLayout {
  return { board: NONE, captured: NONE, statistics: NONE, graph: NONE, comment: NONE, status: NONE, tree: NONE, list: NONE, subBoard: NONE, extra: NONE };
}

/** Rects stacked top to bottom in `area`, sharing its height by the given weights. */
function stack(area: Rect, weights: readonly number[]): Rect[] {
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  let y = area.y;
  return weights.map((weight, index) => {
    const height = index === weights.length - 1 ? area.y + area.height - y : div(area.height * weight, total);
    const part = rect(area.x, y, area.width, height);
    y += height;
    return part;
  });
}

/** The tree on top and the comment under it when both show, as Java shares the tree's room. */
function treeAndComment(area: Rect, show: PanelFlags): Pick<FrameLayout, "tree" | "comment"> {
  if (show.tree && show.comment) {
    const [tree, comment] = stack(area, [1, 1]);
    return { tree: tree!, comment: comment! };
  }
  return { tree: show.tree ? area : NONE, comment: show.comment ? area : NONE };
}

/** The info box, its winrate bar and the graph, top to bottom in `area`, shared by `weights`. */
function infoAndGraph(area: Rect, show: PanelFlags, weights: readonly number[] = [1, 1, 6]): Pick<FrameLayout, "captured" | "statistics" | "graph"> {
  if (show.info === false) return { captured: NONE, statistics: NONE, graph: area };
  const [captured, statistics, graph] = stack(area, weights);
  return { captured: captured!, statistics: statistics!, graph: graph! };
}

/** 精简模式: the board alone, at the saved position. */
function minLayout(width: number, height: number, boardSize: number, position: number): FrameLayout {
  const maxSize = Math.max(Math.min(width, height), boardSize + 5);
  return { ...empty(), board: rect(div(width - maxSize, 8) * position, div(height - maxSize, 2), maxSize, maxSize) };
}

/**
 * 放大小棋盘 (Java's `showLargeSubBoard`): the board on the right; on the left a strip of the info
 * box beside the tree and comment, the graph (sharing its width with the list), and a large
 * sub-board at the bottom.
 */
function largeSubBoardLayout(width: number, height: number, boardSize: number, show: PanelFlags): FrameLayout {
  const maxSize = Math.max(Math.min(width, height), boardSize + 5);
  const margin = Math.trunc(maxSize * 0.02);
  const boardX = width - maxSize - margin;
  const spaceW = Math.max(0, boardX - margin);
  const bottom = show.status ? height - STATUS_HEIGHT : height;
  let panelH = div(height * 2, 7);
  const room = bottom - panelH;
  const length = Math.max(0, Math.min(spaceW, room));
  // A sub-board narrower than its room grows the panels above it, as Java's does.
  if (room > spaceW) panelH += room - spaceW;
  const side = show.tree || show.comment;
  const hasInfo = show.info !== false;
  // The strip above the graph: the info box on the left, the tree and comment beside it.
  const strip = hasInfo || side ? Math.trunc(panelH * 0.53) : 0;
  const infoW = !hasInfo ? 0 : side ? div(spaceW, 2) : spaceW;
  const captured = infoW ? rect(0, 0, infoW, Math.trunc(panelH * 0.2)) : NONE;
  const statistics = infoW ? rect(0, captured.height, infoW, strip - captured.height) : NONE;
  const sideArea = rect(infoW, 0, spaceW - infoW, strip);
  let graph = rect(0, strip, spaceW, panelH - strip);
  let list = NONE;
  if (show.list) {
    if (show.graph === false) list = graph;
    else {
      list = rect(div(spaceW, 2), graph.y, spaceW - div(spaceW, 2), graph.height);
      graph = { ...graph, width: div(spaceW, 2) };
    }
  }
  return {
    board: rect(boardX, div(height - maxSize, 2), maxSize, maxSize),
    captured,
    statistics,
    graph,
    ...treeAndComment(sideArea, show),
    list,
    status: rect(0, bottom, spaceW, show.status ? STATUS_HEIGHT : 0),
    subBoard: rect(div(spaceW - length, 2), bottom - length, length, length),
    extra: NONE,
  };
}

/**
 * 放大胜率图 (Java's `showLargeWinrate`): the board on the right; on the left the sub-board beside
 * the tree and comment in the top quarter, then the info box and a tall graph. Java hides the
 * candidate list here.
 */
function largeGraphLayout(width: number, height: number, boardSize: number, show: PanelFlags): FrameLayout {
  const maxSize = Math.max(Math.min(width, height), boardSize + 5);
  const margin = Math.trunc(maxSize * 0.02);
  const boardX = width - maxSize - margin;
  const spaceW = Math.max(0, boardX - margin);
  const bottom = show.status ? height - STATUS_HEIGHT : height;
  const panelH = div(height, 4);
  const panelW = show.subBoard ? div(spaceW, 2) : 0;
  const length = Math.max(0, Math.min(panelW, panelH));
  const lower = rect(0, panelH, spaceW, bottom - panelH);
  const parts = show.info === false ? { captured: NONE, statistics: NONE, graph: lower } : (() => {
    const row = Math.trunc(lower.height * 0.15);
    return { captured: rect(0, panelH, spaceW, row), statistics: rect(0, panelH + row, spaceW, row), graph: rect(0, panelH + 2 * row, spaceW, lower.height - 2 * row) };
  })();
  return {
    board: rect(boardX, div(height - maxSize, 2), maxSize, maxSize),
    ...parts,
    ...treeAndComment(rect(panelW, 0, spaceW - panelW, panelH), show),
    list: NONE,
    status: rect(0, bottom, spaceW, show.status ? STATUS_HEIGHT : 0),
    subBoard: show.subBoard ? rect(div(panelW - length, 2), 0, length, length) : NONE,
    extra: NONE,
  };
}

/**
 * 思考模式 and 四方图模式: a large square on the left (the plain board, or the four small boards),
 * the board in the room left over, and the panels between them; when that room is narrow the
 * board goes to the bottom with the panels above it (Java's `isWidth`).
 */
function sideBoardLayout(width: number, height: number, boardSize: number, show: PanelFlags): FrameLayout {
  if (width < height) {
    // Upright windows put the square on top and lay out the rest as usual below it.
    const side = Math.min(width, div(height, 2));
    const below = inFrameLayout(width, height - side, boardSize, { ...show, subBoard: false });
    const shift = (area: Rect) => ({ ...area, y: area.y + side });
    const moved = Object.fromEntries(Object.entries(below).map(([name, area]) => [name, shift(area)])) as unknown as FrameLayout;
    return { ...moved, subBoard: NONE, extra: rect(div(width - side, 2), 0, side, side) };
  }
  // Java's square is as tall as the panel; a squarish window keeps 40% of its width for the rest.
  const side = Math.min(Math.max(height, boardSize + 5), Math.trunc(width * 0.6));
  const restW = width - side;
  const bottom = show.status ? height - STATUS_HEIGHT : height;
  const lists = [show.tree ? "tree" : null, show.list ? "list" : show.comment ? "comment" : null].filter((name): name is "tree" | "list" | "comment" => name !== null);
  const layout: FrameLayout = { ...empty(), extra: rect(0, 0, side, side) };
  if (restW * 0.72 > height) {
    const maxSize = Math.max(Math.min(restW, height), boardSize + 5);
    layout.board = rect(width - maxSize, height - maxSize, maxSize, maxSize);
    const column = rect(side, 0, layout.board.x - side, bottom);
    const [upper, lower] = lists.length > 0 ? stack(column, [1, 1]) : [column, NONE];
    Object.assign(layout, infoAndGraph(upper!, show));
    stack(lower!, lists.map(() => 1)).forEach((area, index) => (layout[lists[index]!] = area));
    layout.status = rect(side, bottom, column.width, show.status ? STATUS_HEIGHT : 0);
  } else {
    const maxSize = Math.max(Math.min(restW, Math.trunc(0.77 * height)), boardSize + 5);
    layout.board = rect(side + div(restW - maxSize, 2), height - maxSize, maxSize, maxSize);
    const above = rect(side, 0, restW, layout.board.y);
    const half = lists.length > 0 ? div(above.width, 2) : above.width;
    // A short strip: the info box and its bar need more of it than in a tall column.
    Object.assign(layout, infoAndGraph(rect(above.x, 0, half, above.height), show, [2, 2, 4]));
    stack(rect(above.x + half, 0, above.width - half, above.height), lists.map(() => 1)).forEach((area, index) => (layout[lists[index]!] = area));
    // No room is left for the status line beside a board this tall.
    layout.status = NONE;
  }
  return layout;
}

/**
 * The main window while the board floats (Java's `Float_Board`, `LizzieFrame` 6927–7095): a left
 * column of the info box, the graph and the comment, its width set by the board position; on the
 * right the tree and the list, the sub-board at the bottom. Either column takes the whole width
 * when the other has nothing to show.
 */
function floatedLayout(width: number, height: number, show: PanelFlags, position: number): FrameLayout {
  const left = [show.info !== false ? "info" : null, show.graph !== false ? "graph" : null, show.comment ? "comment" : null].filter((name): name is "info" | "graph" | "comment" => name !== null);
  const right = [show.tree ? "tree" : null, show.list ? "list" : null].filter((name): name is "tree" | "list" => name !== null);
  const sub = show.subBoard;
  const hasRight = right.length > 0 || sub;
  const columnW = left.length === 0 ? 0 : hasRight ? div(width, 8) * Math.max(1, position) : width;
  const bottom = show.status ? height - STATUS_HEIGHT : height;
  const layout: FrameLayout = { ...empty(), status: rect(0, bottom, columnW || width, show.status ? STATUS_HEIGHT : 0) };
  stack(rect(0, 0, columnW, bottom), left.map((name) => (name === "graph" ? 2 : 1))).forEach((area, index) => {
    const name = left[index]!;
    if (name === "info") {
      const [captured, statistics] = stack(area, [3, 2]);
      Object.assign(layout, { captured, statistics });
    } else layout[name] = area;
  });
  const rightArea = rect(columnW, 0, width - columnW, height);
  const length = sub ? Math.max(0, Math.min(rightArea.width, right.length > 0 ? Math.trunc(rightArea.height * 0.75) : rightArea.height)) : 0;
  if (sub) layout.subBoard = rect(rightArea.x + div(rightArea.width - length, 2), height - length, length, length);
  stack(rect(rightArea.x, 0, rightArea.width, height - length), right.map(() => 1)).forEach((area, index) => (layout[right[index]!] = area));
  return layout;
}

/**
 * The main panel's layout for the chosen mode. Normal is Java's `InFrameLayout` at the saved board
 * position; the enlarged layouts apply to landscape panels only, as in Java, and only when there is
 * room for them beside the board.
 */
export function frameLayout(width: number, height: number, boardSize: number, show: PanelFlags, arrangement: Arrangement = DEFAULT_ARRANGEMENT): FrameLayout {
  const position = Math.max(0, Math.min(8, Math.round(arrangement.position)));
  if (arrangement.boardFloated) return floatedLayout(width, height, show, position);
  switch (arrangement.mode) {
    case "Min":
      return minLayout(width, height, boardSize, position);
    case "Thinking":
    case "MultiView":
      return sideBoardLayout(width, height, boardSize, show);
    case "Normal":
      break;
  }
  // The enlarged layouts need room beside a full-height board: a quarter of it at least.
  const side = Math.max(Math.min(width, height), boardSize + 5);
  const roomy = width >= height && width - side * 1.04 >= side / 4;
  if (roomy && arrangement.largeSubBoard && show.subBoard) return largeSubBoardLayout(width, height, boardSize, show);
  if (roomy && arrangement.largeGraph && show.graph !== false) return largeGraphLayout(width, height, boardSize, show);
  return inFrameLayout(width, height, boardSize, show, position);
}
