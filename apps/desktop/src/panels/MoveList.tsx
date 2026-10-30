import { useEffect, useRef } from "react";
import type { GraphPointView } from "@/api/backend";

export interface MoveListLabels {
  label: string;
  variations: string;
}

export interface MoveListProps {
  points: readonly GraphPointView[];
  currentIndex: number;
  /** Children of the current node, including the one on this line. */
  variationCount: number;
  labels: MoveListLabels;
  onJump(nodeId: number): void;
}

/**
 * The displayed line as a column of moves, like the trunk of LizzieYzy Next's
 * variation tree. Branches join once the backend sends the whole tree.
 */
export function MoveList({ points, currentIndex, variationCount, labels, onJump }: MoveListProps) {
  const list = useRef<HTMLOListElement>(null);

  useEffect(() => {
    const current = list.current?.querySelector<HTMLElement>("[aria-current='step']");
    current?.scrollIntoView?.({ block: "nearest" });
  }, [currentIndex, points.length]);

  return (
    <section className="move-list" aria-label={labels.label} data-testid="move-list">
      <ol ref={list}>
        {points.map((point, index) => (
          <li key={point.node_id}>
            <button
              type="button"
              aria-current={index === currentIndex ? "step" : undefined}
              data-analysed={point.summary !== null}
              onClick={() => index !== currentIndex && onJump(point.node_id)}
            >
              <span className="move-dot" aria-hidden="true" />
              <span className="move-index">{point.move_number}</span>
              {index === currentIndex && variationCount > 1 ? (
                <span className="move-branches">{labels.variations.replace("{n}", String(variationCount))}</span>
              ) : null}
            </button>
          </li>
        ))}
      </ol>
    </section>
  );
}
