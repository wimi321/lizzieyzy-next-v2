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

/** The panels that move the others when shown or hidden; the info box and graph only vanish. */
export interface PanelFlags {
  comment: boolean;
  subBoard: boolean;
  tree: boolean;
  status: boolean;
}

export const ALL_PANELS: PanelFlags = { comment: true, subBoard: true, tree: true, status: true };

/**
 * Java's `InFrameLayout` for a landscape window in the default mode, without the candidate list.
 * The comment takes the sub-board's place under the graph when shown, pushing the sub-board under
 * the tree; hidden panels give their room to these neighbours as Java's do. Integer arithmetic
 * follows Java's.
 */
export function inFrameLayout(width: number, height: number, boardSize = 19, show: PanelFlags = ALL_PANELS): FrameLayout {
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

  const ponderingY = show.status ? height - STATUS_HEIGHT : height;
  let subBoardY = gry + grh;
  const subBoardWidth = capw;
  const subBoardHeight = ponderingY - subBoardY;
  let subBoardLength = Math.min(subBoardWidth, subBoardHeight);
  let subBoardX = div(capw - subBoardLength, 2);

  let treeHeight = vh;
  let comment: Rect = { x: vx, y: 0, width: vw, height: vh };
  if (show.comment) {
    if (show.tree) {
      treeHeight = div(vh, 2);
      comment = { x: vx, y: treeHeight, width: vw, height: treeHeight };
    }
    // The comment and the sub-board swap places.
    const right = comment;
    comment = {
      x: subBoardWidth > subBoardHeight ? subBoardX - div(subBoardWidth - subBoardHeight, 2) : subBoardX,
      y: subBoardY,
      width: subBoardWidth,
      height: subBoardHeight,
    };
    subBoardX = right.x;
    subBoardY = right.y;
    subBoardLength = Math.min(right.width, right.height);
  }
  if (show.subBoard && show.comment) {
    treeHeight = treeHeight + div(vh, 2) - subBoardLength;
    subBoardY = Math.max(show.tree ? subBoardY + div(vh, 2) - subBoardLength : subBoardY + vh - subBoardLength, 0);
  }
  if (show.tree && show.subBoard !== show.comment) treeHeight = vh;

  return {
    board: { x: boardX, y: boardY, width: maxSize, height: maxSize },
    captured: { x: 0, y: 0, width: capw, height: caph },
    statistics: { x: 0, y: staty, width: capw, height: stath },
    graph: { x: 0, y: gry, width: capw, height: grh },
    comment: { ...comment, height: Math.max(0, comment.height) },
    status: { x: 0, y: ponderingY, width: capw, height: show.status ? STATUS_HEIGHT : 0 },
    tree: { x: vx, y: 0, width: vw, height: treeHeight },
    subBoard: { x: subBoardX, y: subBoardY, width: Math.max(0, subBoardLength), height: Math.max(0, subBoardLength) },
  };
}
