import { type ReactNode, useEffect, useState } from "react";
import type { AnalysisView, Backend, CandidateView, DocumentView, PointView } from "@/api/backend";

export interface BoardMenuLabels {
  insertBlack: string;
  insertWhite: string;
  addBranch: string;
  previousMove: string;
  findStone: string;
  moveStone: string;
  switchStone: string;
  deleteStone: string;
  review: string;
}

type MenuBackend = Pick<Backend, "insertMove" | "switchStone" | "deleteStone" | "addVariation" | "stoneOrigin" | "goTo" | "step">;

interface Opened {
  point: PointView;
  x: number;
  y: number;
}

interface Item {
  id: string;
  label: string;
  run(): void;
}

function report(error: unknown) {
  console.error("Board menu command failed", error);
}

/**
 * The entries Java's board menus offer at a point (`RightClickMenu` on an empty point,
 * `RightClickMenu2` on a stone). A candidate adds its line as a branch.
 */
export function boardMenuItems(
  document: DocumentView,
  analysis: AnalysisView | null,
  point: PointView,
  labels: BoardMenuLabels,
  run: {
    insert(color: "Black" | "White"): void;
    addBranch(candidate: CandidateView): void;
    previous(): void;
    find(): void;
    move(): void;
    switchStone(): void;
    deleteStone(): void;
    review(): void;
  },
): Item[] {
  const board = document.board;
  if (!board) return [];
  if (board.stones[point.y * board.width + point.x]) {
    return [
      { id: "find-stone", label: labels.findStone, run: run.find },
      { id: "move-stone", label: labels.moveStone, run: run.move },
      { id: "switch-stone", label: labels.switchStone, run: run.switchStone },
      { id: "delete-stone", label: labels.deleteStone, run: run.deleteStone },
      { id: "review-stone", label: labels.review, run: run.review },
    ];
  }
  const candidate =
    analysis && analysis.node_id === document.tree.current
      ? analysis.candidates.find((c) => c.point?.x === point.x && c.point?.y === point.y && c.pv.length > 0)
      : undefined;
  return [
    ...(candidate ? [{ id: "add-branch", label: labels.addBranch, run: () => run.addBranch(candidate) }] : []),
    { id: "insert-black", label: labels.insertBlack, run: () => run.insert("Black") },
    { id: "insert-white", label: labels.insertWhite, run: () => run.insert("White") },
    { id: "previous-move", label: labels.previousMove, run: run.previous },
  ];
}

/** The board's right-click menu: opened at the pointer, closed by a choice, a click elsewhere or Esc. */
export function useBoardMenu(
  backend: MenuBackend,
  document: DocumentView | null,
  analysis: AnalysisView | null,
  revision: number,
  labels: BoardMenuLabels,
  onMove: (from: PointView) => void,
): { open(point: PointView, x: number, y: number): void; menu: ReactNode } {
  const [opened, setOpened] = useState<Opened | null>(null);

  useEffect(() => {
    if (!opened) return;
    const close = (event: Event) => {
      if (event instanceof KeyboardEvent && event.key !== "Escape") return;
      if (event.target instanceof Element && event.target.closest(".board-menu") && event.type !== "keydown") return;
      setOpened(null);
    };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", close);
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("keydown", close);
      window.removeEventListener("blur", close);
    };
  }, [opened]);

  // The stone's own move, checked against the game as it is now.
  const origin = (point: PointView, go: (node: number, parent: number | null) => void) => {
    backend
      .stoneOrigin(point)
      .then((found) => {
        if (found && found.revision === revision) go(found.node_id, found.review_parent);
      })
      .catch(report);
  };

  let menu: ReactNode = null;
  if (opened && document) {
    const { point } = opened;
    const items = boardMenuItems(document, analysis, point, labels, {
      insert: (color) => void backend.insertMove({ type: "Play", data: { color, point } }).catch(report),
      addBranch: (candidate) => void backend.addVariation(candidate.pv).catch(report),
      previous: () => void backend.step(-1).catch(report),
      find: () => origin(point, (node) => void backend.goTo(node).catch(report)),
      move: () => onMove(point),
      switchStone: () => void backend.switchStone(point).catch(report),
      deleteStone: () => void backend.deleteStone(point).catch(report),
      // Java's 回看 starts just before the stone was played.
      review: () => origin(point, (node, parent) => void backend.goTo(parent ?? node).catch(report)),
    });
    menu = (
      <div className="menu-popup board-menu" role="menu" data-testid="board-menu" style={{ left: opened.x, top: opened.y }}>
        {items.map((item) => (
          <div
            key={item.id}
            className="menu-row"
            role="menuitem"
            data-menu-id={item.id}
            onClick={() => {
              setOpened(null);
              item.run();
            }}
          >
            <span className="menu-mark" aria-hidden="true" />
            <span className="menu-label">{item.label}</span>
            <span className="menu-chevron" aria-hidden="true" />
          </div>
        ))}
      </div>
    );
  }

  return { open: (point, x, y) => setOpened({ point, x, y }), menu };
}
