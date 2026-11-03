export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Where LizzieYzy Next puts each panel of its main window, in panel pixels. */
export interface FrameLayout {
  board: Rect;
  /** Players, captures and komi. */
  captured: Rect;
  /** The winrate bar and the lead line under it. */
  statistics: Rect;
  graph: Rect;
  comment: Rect;
  /** The engine status line at the bottom of the left column. */
  status: Rect;
  tree: Rect;
  subBoard: Rect;
}

/** Height Java reserves for the status line: one line of 14 px text at 1.8 line height, plus 4. */
export const STATUS_HEIGHT = Math.round(14 * 1.8) + 4;

/** Java's `BoardPositionProportion`: the board sits at 4/8 of the space left beside it. */
const BOARD_POSITION = 4;

const div = (a: number, b: number) => Math.trunc(a / b);

/**
 * Java's `InFrameLayout` for a landscape window in the default mode, with the panels LizzieYzy Next
 * opens by default except the candidate list: captures, winrate graph, comment, variation tree,
 * sub-board and status line. Integer arithmetic follows Java's.
 */
export function inFrameLayout(width: number, height: number, boardSize = 19): FrameLayout {
  const maxSize = Math.max(Math.min(width, height), boardSize + 5);
  const panelMargin = Math.trunc(maxSize * 0.02);
  const boardX = div(width - maxSize, 8) * BOARD_POSITION;
  const boardY = div(height - maxSize, 2);

  const capw = boardX - panelMargin;
  const caph = boardY + div(maxSize, 8);
  const staty = caph;
  const stath = div(maxSize, 10);
  const gry = staty + stath;
  const grh = div(maxSize, 3);

  const vx = boardX + maxSize + panelMargin;
  const vw = width - vx;
  const vh = height;

  const ponderingY = height - STATUS_HEIGHT;
  // With the comment shown, the comment takes the sub-board's place in the left column and the
  // sub-board moves under the tree, filling the right column's lower half from the bottom.
  const commentY = gry + grh;
  const half = div(vh, 2);
  const subLength = Math.min(vw, half);
  const treeHeight = half + half - subLength;
  const subY = Math.max(half + half - subLength, 0);

  return {
    board: { x: boardX, y: boardY, width: maxSize, height: maxSize },
    captured: { x: 0, y: 0, width: capw, height: caph },
    statistics: { x: 0, y: staty, width: capw, height: stath },
    graph: { x: 0, y: gry, width: capw, height: grh },
    comment: { x: 0, y: commentY, width: capw, height: Math.max(0, ponderingY - commentY) },
    status: { x: 0, y: ponderingY, width: capw, height: STATUS_HEIGHT },
    tree: { x: vx, y: 0, width: vw, height: treeHeight },
    subBoard: { x: vx, y: subY, width: subLength, height: subLength },
  };
}
